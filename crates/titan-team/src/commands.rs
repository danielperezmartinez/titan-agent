//! Every command except `ask`: `check`, `roles`, `new`, `show`, `diff`,
//! `test`, `decide` and `close`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;
use titan_agent_core::team::{DECISION_LIMIT, EventKind, RoleInfo, TeamInfo, clip};

use crate::config::{ConfigError, TeamConfig, resolve};
use crate::process::{git, run_shell};
use crate::state::{self, EventLog, TaskState, check_task_name, now_iso, relative, task_dir};
use crate::{Args, Failure};

/// Prints the errors of an invalid `team.toml` and returns exit code 2.
pub fn report_invalid(command: &str, args: &Args, errors: &[ConfigError]) -> i32 {
    if command == "check" && args.flag("--json") {
        println!("{}", json!({"ok": false, "errors": errors}));
    } else {
        eprintln!("team.toml no es válido:");
        for error in errors {
            eprintln!("{}: {} ({})", error.path, error.message, error.code);
        }
    }
    2
}

pub fn check(team: &TeamConfig, args: &Args) -> Result<i32, Failure> {
    if args.flag("--json") {
        println!("{}", json!({"ok": true, "errors": []}));
    } else {
        println!(
            "team.toml válido: equipo «{}» con {} roles ({})",
            team.name,
            team.roles.len(),
            team.roles
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(0)
}

pub fn roles(team: &TeamConfig, args: &Args) -> Result<i32, Failure> {
    let flow = &team.flow;
    if args.flag("--json") {
        let roles: serde_json::Map<String, serde_json::Value> = team
            .roles
            .iter()
            .map(|r| {
                (
                    r.name.clone(),
                    json!({"cli": r.cli, "model": r.model, "effort": r.effort,
                        "sandbox": r.sandbox, "launch": r.launch, "purpose": r.purpose}),
                )
            })
            .collect();
        let value = json!({"roles": roles, "flow": {"steps": flow.steps,
            "review_role": flow.review_role, "fix_role": flow.fix_role,
            "max_review_rounds": flow.max_review_rounds}});
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
        return Ok(0);
    }
    println!("ROL\tCLI\tMODELO\tESFUERZO\tSANDBOX\tPROPÓSITO");
    for r in &team.roles {
        let sandbox = if r.cli == "codex" {
            r.sandbox.as_str()
        } else {
            "-"
        };
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            r.name,
            r.cli,
            r.model,
            r.effort.as_deref().unwrap_or("-"),
            sandbox,
            r.purpose
        );
    }
    println!("Flujo: {}", flow.steps.join(" → "));
    println!(
        "Revisor: {} · corrector: {} · máximo rondas: {}",
        flow.review_role.as_deref().unwrap_or("-"),
        flow.fix_role.as_deref().unwrap_or("-"),
        flow.max_review_rounds
    );
    Ok(0)
}

fn task_arg(rest: &[String]) -> Result<&str, Failure> {
    let task = rest
        .first()
        .ok_or_else(|| Failure::usage("falta el nombre del encargo"))?;
    check_task_name(task)?;
    Ok(task)
}

pub fn new(team: &TeamConfig, rest: &[String], args: &Args, cwd: &Path) -> Result<i32, Failure> {
    let task = task_arg(rest)?;
    if state::state_file(team, task).exists() {
        return Err(Failure::usage(format!("Ya existe el encargo {task}")));
    }
    let repo = &team.project.repo;
    if !repo.is_dir() {
        return Err(Failure::usage(format!(
            "No existe el repositorio configurado: {}",
            repo.display()
        )));
    }
    let brief_src = resolve(
        cwd,
        Path::new(
            args.option(&["--brief"])
                .ok_or_else(|| Failure::usage("new necesita --brief FICHERO"))?,
        ),
    );
    if !brief_src.is_file() {
        return Err(Failure::usage(format!(
            "No existe el brief: {}",
            brief_src.display()
        )));
    }
    let base = args
        .option(&["--base"])
        .map(str::to_string)
        .unwrap_or_else(|| team.project.base.clone());
    let commit = git(repo, &["rev-parse", &base])?.trim().to_string();
    let worktree = team.path(&team.project.worktrees_dir.join(task));
    let branch = format!("{}{task}", team.project.branch_prefix);
    if let Some(parent) = worktree.parent() {
        std::fs::create_dir_all(parent).map_err(Failure::io)?;
    }
    let wt = worktree.to_string_lossy().into_owned();
    git(repo, &["worktree", "add", "-b", &branch, &wt, &base])?;
    if !team.project.hidden_paths.is_empty() {
        let mut sparse = vec!["sparse-checkout", "set", "--no-cone", "/*"];
        let hidden: Vec<String> = team
            .project
            .hidden_paths
            .iter()
            .map(|p| format!("!{p}"))
            .collect();
        sparse.extend(hidden.iter().map(String::as_str));
        git(&worktree, &sparse)?;
    }
    let dir = task_dir(team, task);
    std::fs::create_dir_all(&dir).map_err(Failure::io)?;
    let brief = dir.join("encargo.md");
    std::fs::copy(&brief_src, &brief).map_err(Failure::io)?;
    let state = TaskState {
        task: task.to_string(),
        created: now_iso(),
        repo: relative(&team.base, repo),
        worktree: relative(&team.base, &worktree),
        branch: branch.clone(),
        base: base.clone(),
        base_commit: commit.clone(),
        brief: relative(&team.base, &brief),
        review_rounds: 0,
        roles: Default::default(),
        history: Vec::new(),
        closed: false,
        closed_at: None,
    };
    state::save(team, &state)?;
    EventLog::new(team, task).append(EventKind::TaskCreated {
        base,
        base_commit: commit,
        branch: branch.clone(),
        worktree: state.worktree.clone(),
        team: team_info(team),
    });
    println!(
        "Worktree: {}\nRama: {branch}\nSiguiente paso: titan-team ask <rol> {task}  (primer rol de flow.steps)",
        worktree.display()
    );
    Ok(0)
}

fn team_info(team: &TeamConfig) -> TeamInfo {
    TeamInfo {
        name: team.name.clone(),
        director: team.director().map(str::to_string),
        roles: team
            .roles
            .iter()
            .map(|r| RoleInfo {
                name: r.name.clone(),
                cli: r.cli.clone(),
                model: r.model.clone(),
                effort: r.effort.clone(),
                sandbox: (r.cli == "codex").then(|| r.sandbox.clone()),
            })
            .collect(),
        steps: team.flow.steps.clone(),
        review_role: team.flow.review_role.clone(),
        max_review_rounds: team.flow.max_review_rounds,
    }
}

pub fn worktree(team: &TeamConfig, state: &TaskState) -> PathBuf {
    team.path(Path::new(&state.worktree))
}

pub fn show(team: &TeamConfig, rest: &[String], args: &Args) -> Result<i32, Failure> {
    let state = state::load(team, task_arg(rest)?)?;
    if args.flag("--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&state).unwrap_or_default()
        );
        return Ok(0);
    }
    println!(
        "Encargo: {}\nWorktree: {}\nRama: {}\nRevisiones: {}/{}{}",
        state.task,
        state.worktree,
        state.branch,
        state.review_rounds,
        team.flow.max_review_rounds,
        if state.closed { "\nCerrado" } else { "" }
    );
    for (name, info) in &state.roles {
        println!(
            "Rol {name}: hilo {} · turnos {}",
            info.thread_id.as_deref().unwrap_or("-"),
            info.turns
        );
    }
    for h in &state.history {
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
    Ok(0)
}

pub fn diff(team: &TeamConfig, rest: &[String], args: &Args) -> Result<i32, Failure> {
    let state = state::load(team, task_arg(rest)?)?;
    let range = format!("{}..HEAD", state.base_commit);
    let mut git_args = vec!["diff"];
    if args.flag("--stat") {
        git_args.push("--stat");
    }
    git_args.push(&range);
    print!("{}", git(&worktree(team, &state), &git_args)?);
    Ok(0)
}

pub fn test(team: &TeamConfig, rest: &[String]) -> Result<i32, Failure> {
    let state = state::load(team, task_arg(rest)?)?;
    let (code, output) = run_tests(team, &state)?;
    println!("exit: {code}\n{output}");
    Ok(code)
}

/// Runs `project.test_cmd` in the task's worktree.
pub fn run_tests(team: &TeamConfig, state: &TaskState) -> Result<(i32, String), Failure> {
    let command = team
        .project
        .test_cmd
        .as_deref()
        .ok_or_else(|| Failure::usage("No hay project.test_cmd configurado"))?;
    Ok(run_shell(
        command,
        &worktree(team, state),
        Duration::from_secs(team.project.test_timeout_s),
    ))
}

pub fn decide(team: &TeamConfig, rest: &[String], args: &Args) -> Result<i32, Failure> {
    let task = task_arg(rest)?;
    let state = state::load(team, task)?;
    let text = args
        .option(&["-m", "--message"])
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| Failure::usage("decide necesita -m con el texto de la decisión"))?;
    EventLog::new(team, &state.task).append(EventKind::Decision {
        by: team.director().map(str::to_string),
        text: clip(text, DECISION_LIMIT),
    });
    println!("Decisión anotada en {task}");
    Ok(0)
}

pub fn close(team: &TeamConfig, rest: &[String], args: &Args) -> Result<i32, Failure> {
    let task = task_arg(rest)?;
    let mut state = state::load(team, task)?;
    let wt = worktree(team, &state).to_string_lossy().into_owned();
    git(&team.project.repo, &["worktree", "remove", &wt])?;
    let delete_branch = args.flag("--delete-branch");
    if delete_branch {
        git(&team.project.repo, &["branch", "-D", &state.branch])?;
    }
    state.closed = true;
    state.closed_at = Some(now_iso());
    state::save(team, &state)?;
    EventLog::new(team, task).append(EventKind::TaskClosed { delete_branch });
    println!("Encargo {task} cerrado");
    Ok(0)
}
