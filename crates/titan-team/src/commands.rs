//! Every command except `ask`.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command as Process, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;
use titan_agent_core::team::events::{EventKind, RoleInfo};

use crate::args::Command;
use crate::context::{Context, normalize, now_iso};
use crate::git::{git, run as git_run};
use crate::log::EventLog;
use crate::state::{self, TaskState, check_task_name};
use crate::{Result, TeamError, ask};

/// Lines of test output kept (the tail).
const TEST_LINES: usize = 150;

pub fn run(ctx: &Context, command: Command) -> Result<u8> {
    match command {
        Command::Roles { json } => roles(ctx, json),
        Command::Check => check(ctx),
        Command::New { task, brief, base } => new(ctx, &task, &brief, base.as_deref()),
        Command::Ask(args) => ask::ask(ctx, &args),
        Command::Show { task, json } => show(ctx, &task, json),
        Command::Diff { task, stat } => diff(ctx, &task, stat),
        Command::Test { task } => {
            let task = state::load(ctx, &task)?;
            let (code, output) = run_tests(ctx, &task)?;
            println!("exit: {code}\n{output}");
            Ok(u8::try_from(code.clamp(0, 255)).unwrap_or(1))
        }
        Command::Note { task, text } => {
            state::load(ctx, &task)?;
            EventLog::new(ctx, &task).emit(EventKind::Note {
                text: titan_agent_core::team::events::label(&text),
            });
            println!("Nota anotada en {task}");
            Ok(0)
        }
        Command::Close {
            task,
            delete_branch,
        } => close(ctx, &task, delete_branch),
        Command::Help | Command::Version | Command::Doc | Command::Init { .. } => Ok(0),
    }
}

fn roles(ctx: &Context, as_json: bool) -> Result<u8> {
    let c = &ctx.config;
    if as_json {
        let value = json!({"roles": c.roles, "flow": c.flow});
        println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("roles serialize")
        );
        return Ok(0);
    }
    println!("ROL\tCLI\tMODELO\tESFUERZO\tSANDBOX\tPROPÓSITO");
    for (name, role) in &c.roles {
        println!(
            "{name}\t{}\t{}\t{}\t{}\t{}",
            role.cli,
            role.model,
            role.effort.as_deref().unwrap_or("-"),
            if role.launch { role.sandbox() } else { "-" },
            role.purpose.as_deref().unwrap_or("")
        );
    }
    println!("Flujo: {}", c.flow.steps.join(" → "));
    println!(
        "Revisor: {} · corrector: {} · máximo rondas: {}",
        c.flow.review_role.as_deref().unwrap_or("-"),
        c.flow.fix_role.as_deref().unwrap_or("-"),
        c.flow.max_review_rounds
    );
    Ok(0)
}

/// Validates the configuration (already parsed) and the files it names.
fn check(ctx: &Context) -> Result<u8> {
    let mut problems = Vec::new();
    for (name, role) in &ctx.config.roles {
        for (key, file) in [
            ("prompt_file", &role.prompt_file),
            ("output_schema", &role.output_schema),
        ] {
            if let Some(file) = file {
                let path = ctx.path(file);
                if !path.is_file() {
                    problems.push(format!(
                        "[roles.{name}].{key}: no existe {}",
                        path.display()
                    ));
                } else if key == "output_schema"
                    && std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                        .is_none()
                {
                    problems.push(format!("[roles.{name}].output_schema: no es JSON válido"));
                }
            }
        }
    }
    if !ctx.repo().is_dir() {
        problems.push(format!(
            "[project].repo: no existe {}",
            ctx.repo().display()
        ));
    }
    if !problems.is_empty() {
        return Err(TeamError::new(format!(
            "{}:\n- {}",
            ctx.config_path.display(),
            problems.join("\n- ")
        )));
    }
    let launchable = ctx.config.roles.values().filter(|r| r.launchable()).count();
    println!(
        "[ok] {} · {} roles ({launchable} se lanzan) · flujo: {}",
        ctx.config_path.display(),
        ctx.config.roles.len(),
        if ctx.config.flow.steps.is_empty() {
            "-".to_string()
        } else {
            ctx.config.flow.steps.join(" → ")
        }
    );
    Ok(0)
}

/// `titan-team init`: a starting `team.toml` in `./.agents/` (or `--config`).
pub fn init(explicit: Option<&str>, force: bool) -> Result<u8> {
    let cwd = std::env::current_dir()
        .map_err(|e| TeamError::new(format!("No se puede leer la carpeta actual: {e}")))?;
    let path = match explicit {
        Some(path) if path.ends_with(".toml") => normalize(&cwd.join(path)),
        Some(dir) => normalize(&cwd.join(dir).join(".agents/team.toml")),
        None => cwd.join(".agents/team.toml"),
    };
    if path.exists() && !force {
        return Err(TeamError::new(format!(
            "Ya existe {}; usa --force para sobrescribirlo",
            path.display()
        )));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| TeamError::new(format!("No se pudo crear {}: {e}", dir.display())))?;
    }
    std::fs::write(&path, include_str!("../team.example.toml"))
        .map_err(|e| TeamError::new(format!("No se pudo escribir {}: {e}", path.display())))?;
    println!(
        "Creado {}. Escribe los prompts que nombra, ajústalo y comprueba con titan-team check (formato: titan-team doc).",
        path.display()
    );
    Ok(0)
}

fn new(ctx: &Context, task: &str, brief: &str, base: Option<&str>) -> Result<u8> {
    check_task_name(task)?;
    let state_file = state::state_file(ctx, task);
    if state_file.exists() {
        return Err(TeamError::new(format!("Ya existe el encargo {task}")));
    }
    let repo = ctx.repo();
    if !repo.is_dir() {
        return Err(TeamError::new(format!(
            "No existe el repositorio configurado: {}",
            repo.display()
        )));
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let brief_src = normalize(&cwd.join(brief));
    if !brief_src.is_file() {
        return Err(TeamError::new(format!(
            "No existe el brief: {}",
            brief_src.display()
        )));
    }
    let project = &ctx.config.project;
    let base = base.unwrap_or(&project.base).to_string();
    let commit = git(&repo, &["rev-parse", &base])?.trim().to_string();
    let wt = ctx.path(&format!("{}/{task}", project.worktrees_dir));
    let branch = format!("{}{task}", project.branch_prefix);
    if let Some(parent) = wt.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| TeamError::new(format!("No se pudo crear {}: {e}", parent.display())))?;
    }
    let wt_text = wt.display().to_string();
    git(&repo, &["worktree", "add", "-b", &branch, &wt_text, &base])?;
    // The roles get only their prompt and the brief: the hidden paths stay
    // in the branch but not in the worktree (sparse checkout).
    if !project.hidden_paths.is_empty() {
        let mut args = vec![
            "sparse-checkout".to_string(),
            "set".into(),
            "--no-cone".into(),
            "/*".into(),
        ];
        args.extend(project.hidden_paths.iter().map(|p| format!("!{p}")));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        git(&wt, &args)?;
    }
    let dir = state::task_dir(ctx, task);
    std::fs::create_dir_all(&dir)
        .map_err(|e| TeamError::new(format!("No se pudo crear {}: {e}", dir.display())))?;
    let brief_dest = dir.join("encargo.md");
    std::fs::copy(&brief_src, &brief_dest)
        .map_err(|e| TeamError::new(format!("No se pudo copiar el brief: {e}")))?;
    let state = TaskState {
        task: task.to_string(),
        created: now_iso(),
        repo: relative_or(ctx, &repo),
        worktree: relative_or(ctx, &wt),
        branch: branch.clone(),
        base: base.clone(),
        base_commit: commit,
        brief: relative_or(ctx, &brief_dest),
        review_rounds: 0,
        roles: Default::default(),
        history: Vec::new(),
        closed: false,
        closed_at: None,
    };
    state::save(ctx, &state)?;
    let c = &ctx.config;
    EventLog::new(ctx, task).emit(EventKind::TaskCreated {
        branch: branch.clone(),
        base,
        base_dir: ctx.base.display().to_string(),
        team: c.name.clone(),
        roles: c
            .roles
            .iter()
            .map(|(name, r)| RoleInfo {
                name: name.clone(),
                label: r.label.clone(),
                cli: r.cli.clone(),
                model: r.model.clone(),
                effort: r.effort.clone(),
                sandbox: r.launch.then(|| r.sandbox().to_string()),
                purpose: r.purpose.clone(),
                launch: r.launch,
            })
            .collect(),
        steps: c.flow.steps.clone(),
        review_role: c.flow.review_role.clone(),
        max_review_rounds: c.flow.max_review_rounds,
    });
    let first = c.flow.steps.first().map(String::as_str).unwrap_or("<rol>");
    println!(
        "Worktree: {}\nRama: {branch}\nSiguiente paso: titan-team ask {first} {task}",
        wt.display()
    );
    Ok(0)
}

fn relative_or(ctx: &Context, path: &std::path::Path) -> String {
    let relative = ctx.relative(path);
    if relative.is_empty() {
        ".".into()
    } else {
        relative
    }
}

fn show(ctx: &Context, task: &str, as_json: bool) -> Result<u8> {
    let state = state::load(ctx, task)?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&state).expect("the task state serializes")
        );
        return Ok(0);
    }
    println!(
        "Encargo: {}\nWorktree: {}\nRama: {}\nRevisiones: {}/{}{}",
        state.task,
        state.worktree,
        state.branch,
        state.review_rounds,
        ctx.config.flow.max_review_rounds,
        if state.closed { "\nCerrado" } else { "" }
    );
    for (name, info) in &state.roles {
        println!(
            "Rol {name}: hilo {} · turnos {}",
            info.thread_id.as_deref().unwrap_or("-"),
            info.turns
        );
    }
    let mut total = titan_agent_core::team::events::Usage::default();
    for h in &state.history {
        total.add(&h.usage);
        println!(
            "{:02} {} turno {} {} {}s tokens {}/{} commit {} avisos {} salida {}",
            h.n,
            h.role,
            h.turn,
            h.status,
            h.duration_s,
            h.usage.input_tokens,
            h.usage.output_tokens,
            h.commit.as_deref().unwrap_or("-"),
            if h.warnings.is_empty() {
                "-".to_string()
            } else {
                h.warnings.join(", ")
            },
            h.output
        );
    }
    println!(
        "Total: {} llamadas · {} entrada ({} en caché) · {} salida",
        state.history.len(),
        total.input_tokens,
        total.cached_input_tokens,
        total.output_tokens
    );
    Ok(0)
}

fn diff(ctx: &Context, task: &str, stat: bool) -> Result<u8> {
    let state = state::load(ctx, task)?;
    let range = format!("{}..HEAD", state.base_commit);
    let mut args = vec!["diff"];
    if stat {
        args.push("--stat");
    }
    args.push(&range);
    let output = git_run(&worktree(ctx, &state), &args)?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    Ok(u8::try_from(output.status.code().unwrap_or(1).clamp(0, 255)).unwrap_or(1))
}

fn close(ctx: &Context, task: &str, delete_branch: bool) -> Result<u8> {
    let mut state = state::load(ctx, task)?;
    let repo = ctx.repo();
    let wt = worktree(ctx, &state).display().to_string();
    git(&repo, &["worktree", "remove", &wt])?;
    if delete_branch {
        git(&repo, &["branch", "-D", &state.branch])?;
    }
    state.closed = true;
    state.closed_at = Some(now_iso());
    state::save(ctx, &state)?;
    EventLog::new(ctx, task).emit(EventKind::TaskClosed);
    println!("Encargo {task} cerrado");
    Ok(0)
}

pub fn worktree(ctx: &Context, state: &TaskState) -> PathBuf {
    ctx.path(&state.worktree)
}

/// Runs `project.test_cmd` in the task worktree: `(exit code, last lines)`.
pub fn run_tests(ctx: &Context, state: &TaskState) -> Result<(i32, String)> {
    let command = ctx
        .config
        .project
        .test_cmd
        .as_deref()
        .ok_or_else(|| TeamError::new("No hay project.test_cmd configurado"))?;
    let limit = Duration::from_secs(ctx.config.project.test_timeout_s);
    let (shell, flag) = if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    let mut child = Process::new(shell)
        .args([flag, command])
        .current_dir(worktree(ctx, state))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| TeamError::new(format!("No se pudieron ejecutar los tests: {e}")))?;
    let mut out = child.stdout.take().expect("stdout is piped");
    let mut err = child.stderr.take().expect("stderr is piped");
    let out = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = out.read_to_string(&mut text);
        text
    });
    let err = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = err.read_to_string(&mut text);
        text
    });
    let start = Instant::now();
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code().unwrap_or(1),
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                break 124;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(TeamError::new(e.to_string())),
        }
    };
    let text = out.join().unwrap_or_default() + &err.join().unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let mut tail = lines[lines.len().saturating_sub(TEST_LINES)..].join("\n");
    if code == 124 && start.elapsed() > limit {
        tail.push_str(&format!("\nTests agotaron {} s", limit.as_secs()));
    }
    Ok((code, tail))
}
