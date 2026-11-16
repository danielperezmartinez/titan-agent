//! `titan-team ask`: one turn of a role, in a new thread or continuing its
//! own, with the commit of what it wrote and the events for the monitor.

use std::time::{Duration, Instant};

use serde_json::Value;
use titan_agent_core::team::config::path_warnings;
use titan_agent_core::team::events::{EventKind, TurnStatus, label};

use crate::args::AskArgs;
use crate::codex;
use crate::commands::{run_tests, worktree};
use crate::context::{Context, now_iso};
use crate::git::{git, has_identity, run as git_run};
use crate::log::EventLog;
use crate::state::{self, RoleState, Turn};
use crate::{Result, TeamError};

/// Largest diff appended to a prompt with `--diff`.
const DIFF_CHARS: usize = 60_000;
/// Shortest time between two `action` events of a turn.
const ACTION_EVERY: Duration = Duration::from_secs(2);

pub fn ask(ctx: &Context, args: &AskArgs) -> Result<u8> {
    let role = ctx
        .config
        .roles
        .get(&args.role)
        .ok_or_else(|| TeamError::new(format!("Rol inexistente: {}", args.role)))?;
    if !role.launch {
        return Err(TeamError::new(format!(
            "El rol {} tiene launch=false y no se puede encargar",
            args.role
        )));
    }
    // The tool never launches Claude Code (claude -p): ADR-0001 and ADR-0005.
    if role.cli != "codex" {
        return Err(TeamError::new(format!(
            "El rol {} usa cli={}; titan-team solo lanza roles codex",
            args.role, role.cli
        )));
    }
    let mut task = state::load(ctx, &args.task)?;
    if task.closed {
        return Err(TeamError::new(format!(
            "El encargo {} está cerrado",
            args.task
        )));
    }
    let wt = worktree(ctx, &task);
    if !wt.is_dir() {
        return Err(TeamError::new(format!(
            "No existe el worktree: {}",
            wt.display()
        )));
    }
    let info = task.roles.get(&args.role).cloned();
    let thread = info
        .as_ref()
        .and_then(|i| i.thread_id.clone())
        .filter(|_| !args.new);
    if thread.is_some() && args.message.is_none() && args.file.is_none() {
        return Err(TeamError::new(
            "Para continuar un hilo es obligatorio indicar -m o -f",
        ));
    }
    let message = match &args.file {
        Some(file) => Some(
            std::fs::read_to_string(file)
                .map_err(|e| TeamError::new(format!("No se pudo leer el mensaje: {e}")))?,
        ),
        None => args.message.clone(),
    };
    let mut prompt = match &thread {
        Some(_) => format!(
            "# Mensaje del director\n\n{}",
            message.clone().unwrap_or_default()
        ),
        None => {
            let prompt_file = ctx.path(role.prompt_file.as_deref().unwrap_or_default());
            let brief_file = ctx.path(&task.brief);
            let read = |p: &std::path::Path| {
                std::fs::read_to_string(p).map_err(|e| {
                    TeamError::new(format!(
                        "No se pudo leer el prompt o encargo ({}): {e}",
                        p.display()
                    ))
                })
            };
            let mut text = format!(
                "{}\n\n# Encargo\n\n{}",
                read(&prompt_file)?,
                read(&brief_file)?
            );
            if let Some(message) = &message {
                text.push_str(&format!("\n\n# Mensaje del director\n\n{message}"));
            }
            text
        }
    };
    let mut warnings = Vec::new();
    if args.diff {
        let mut diff = git(&wt, &["diff", &format!("{}..HEAD", task.base_commit)])?;
        if diff.chars().count() > DIFF_CHARS {
            diff = diff.chars().take(DIFF_CHARS).collect();
            warnings.push("Diff truncado a 60 000 caracteres".to_string());
        }
        prompt.push_str(&format!("\n\n# Diff respecto a la base\n\n{diff}"));
    }
    if args.test {
        let (code, output) = run_tests(ctx, &task)?;
        prompt.push_str(&format!(
            "\n\n# Resultado de los tests (exit {code})\n\n{output}"
        ));
    }
    let flow = &ctx.config.flow;
    let is_reviewer = flow.review_role.as_deref() == Some(args.role.as_str());
    let next_round = task.review_rounds + u32::from(is_reviewer);
    if is_reviewer && next_round > flow.max_review_rounds && !args.force {
        return Err(TeamError::with_code(
            format!(
                "Se superó el máximo de rondas de revisión ({}); usa --force",
                flow.max_review_rounds
            ),
            3,
        ));
    }
    let n = task.history.len() as u32 + 1;
    let dir = state::task_dir(ctx, &args.task);
    let out_file = dir.join(state::output_name(n, &args.role, "response"));
    let schema = role.output_schema.as_deref().map(|s| ctx.path(s));
    let argv = codex::command(
        role,
        thread.as_deref(),
        &out_file,
        &wt,
        &codex::overrides(&ctx.config, role),
        schema.as_deref(),
    );
    if args.dry_run {
        println!("Comando: {}", codex::shell_line(&argv));
        println!("\n--- prompt ---\n{prompt}");
        return Ok(0);
    }
    if !on_path(&argv[0]) {
        return Err(TeamError::new(format!(
            "{} no está disponible en PATH",
            argv[0]
        )));
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| TeamError::new(format!("No se pudo crear {}: {e}", dir.display())))?;

    let turn = info.as_ref().map_or(0, |i| i.turns) + 1;
    let log = EventLog::new(ctx, &args.task);
    let brief_label = message
        .as_deref()
        .map(label)
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| {
            if thread.is_some() {
                "continúa".into()
            } else {
                "encargo".into()
            }
        });
    log.emit(EventKind::TurnStarted {
        role: args.role.clone(),
        turn,
        resumed: thread.is_some(),
        thread_id: thread.clone(),
        round: is_reviewer.then_some(next_round),
        message: brief_label,
    });
    let started = Instant::now();
    let action_log = log.clone();
    let action_role = args.role.clone();
    let mut last: Option<(Instant, String)> = None;
    let outcome = codex::run(
        &argv,
        &wt,
        &prompt,
        Duration::from_secs(role.timeout_s),
        move |action| {
            let due = last
                .as_ref()
                .is_none_or(|(at, text)| at.elapsed() >= ACTION_EVERY && *text != action);
            if due {
                action_log.emit(EventKind::Action {
                    role: action_role.clone(),
                    text: action.clone(),
                });
                last = Some((Instant::now(), action));
            }
        },
    );
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            log.emit(EventKind::TurnFinished {
                role: args.role.clone(),
                turn,
                status: TurnStatus::Error,
                thread_id: thread.clone(),
                duration_ms: started.elapsed().as_millis() as u64,
                usage: Default::default(),
                commit: None,
                changed_files: 0,
                warnings: 0,
                summary: "[x] no se pudo ejecutar codex".into(),
            });
            return Err(TeamError::new(format!(
                "No se pudo ejecutar {}: {error}",
                argv[0]
            )));
        }
    };
    let write = |suffix: &str, text: &str| {
        let _ = std::fs::write(dir.join(state::output_name(n, &args.role, suffix)), text);
    };
    write("jsonl", &outcome.stdout);
    write("stderr", &outcome.stderr);
    let text = codex::answer(&out_file, &outcome);
    let _ = std::fs::remove_file(&out_file);
    let status = if outcome.timed_out {
        TurnStatus::Timeout
    } else if !outcome.exit_ok || !outcome.errors.is_empty() {
        TurnStatus::Error
    } else {
        TurnStatus::Ok
    };
    let review: Option<Value> = schema
        .as_ref()
        .and_then(|_| serde_json::from_str::<Value>(text.trim()).ok())
        .filter(|v| v.get("verdict").is_some());
    let suffix = if schema.is_some() { "json" } else { "md" };
    let output_path = dir.join(state::output_name(n, &args.role, suffix));
    let _ = std::fs::write(&output_path, &text);

    let mut commit = None;
    let mut changed: Vec<String> = Vec::new();
    if role.sandbox() != "read-only" && ctx.config.project.autocommit {
        git(&wt, &["add", "-A"])?;
        let staged = git_run(&wt, &["diff", "--cached", "--quiet"])?;
        if !staged.status.success() {
            let message = format!("team: {} turno {turn}", args.role);
            let mut commit_args: Vec<&str> = Vec::new();
            if !has_identity(&wt) {
                commit_args.extend(["-c", "user.name=team", "-c", "user.email=team@localhost"]);
            }
            commit_args.extend(["commit", "-m", &message]);
            git(&wt, &commit_args)?;
            commit = Some(
                git(&wt, &["rev-parse", "--short", "HEAD"])?
                    .trim()
                    .to_string(),
            );
            changed = git(&wt, &["diff", "--name-only", "HEAD~1..HEAD"])?
                .lines()
                .map(str::to_string)
                .collect();
        }
    } else if role.sandbox() == "read-only"
        && !git(&wt, &["status", "--porcelain"])?.trim().is_empty()
    {
        warnings.push("El rol read-only dejó cambios en el árbol".into());
    }
    warnings.extend(path_warnings(role, &changed));

    let thread_id = outcome.thread_id.clone().or(thread.clone());
    let summary = summary(
        status,
        &outcome,
        &review,
        commit.as_deref(),
        changed.len(),
        &text,
    );
    log.emit(EventKind::TurnFinished {
        role: args.role.clone(),
        turn,
        status,
        thread_id: thread_id.clone(),
        duration_ms: started.elapsed().as_millis() as u64,
        usage: outcome.usage,
        commit: commit.clone(),
        changed_files: changed.len() as u32,
        warnings: warnings.len() as u32,
        summary,
    });
    if is_reviewer {
        task.review_rounds = next_round;
        if let Some(review) = &review {
            log.emit(EventKind::Review {
                role: args.role.clone(),
                round: next_round,
                max_rounds: flow.max_review_rounds,
                verdict: review["verdict"].as_str().unwrap_or("?").to_string(),
                findings: review["findings"].as_array().map_or(0, |f| f.len() as u32),
            });
        }
    }
    let entry = Turn {
        n,
        ts: now_iso(),
        role: args.role.clone(),
        turn,
        thread_id: thread_id.clone(),
        resumed: thread.is_some(),
        status: status.as_str().into(),
        duration_s: (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        usage: outcome.usage,
        output: ctx.relative(&output_path),
        commit,
        changed_files: changed,
        warnings,
    };
    task.roles.insert(
        args.role.clone(),
        RoleState {
            cli: role.cli.clone(),
            model: role.model.clone(),
            thread_id,
            turns: turn,
        },
    );
    task.history.push(entry.clone());
    state::save(ctx, &task)?;
    if args.json {
        let mut value = serde_json::to_value(&entry).expect("a turn serializes");
        value["text"] = Value::String(text.clone());
        value["parsed"] = review.clone().unwrap_or(Value::Null);
        println!("{value}");
    } else {
        print_report(
            ctx,
            &args.role,
            &entry,
            &text,
            review.as_ref(),
            task.review_rounds,
        );
    }
    Ok(if status == TurnStatus::Ok { 0 } else { 1 })
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| {
            let candidate = dir.join(program);
            candidate.is_file() || (cfg!(windows) && candidate.with_extension("exe").is_file())
        })
    })
}

/// The short answer on the arrow back to the director.
fn summary(
    status: TurnStatus,
    outcome: &codex::Outcome,
    review: &Option<Value>,
    commit: Option<&str>,
    changed: usize,
    text: &str,
) -> String {
    match status {
        TurnStatus::Timeout => return "[x] agotó el tiempo".into(),
        TurnStatus::Error => {
            let first = outcome
                .errors
                .first()
                .cloned()
                .or_else(|| {
                    outcome
                        .stderr
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "error".into());
            return label(&format!("[x] {first}"));
        }
        TurnStatus::Ok => {}
    }
    if let Some(review) = review {
        let findings = review["findings"].as_array().map_or(0, Vec::len);
        return format!(
            "{} · {findings} hallazgo{}",
            review["verdict"].as_str().unwrap_or("?"),
            if findings == 1 { "" } else { "s" }
        );
    }
    if commit.is_some() {
        return format!(
            "[ok] {changed} fichero{}",
            if changed == 1 { "" } else { "s" }
        );
    }
    let first = label(text);
    if first.is_empty() {
        "[ok]".into()
    } else {
        format!("[ok] {first}")
    }
}

fn print_report(
    ctx: &Context,
    name: &str,
    entry: &Turn,
    text: &str,
    review: Option<&Value>,
    rounds: u32,
) {
    let role = &ctx.config.roles[name];
    let u = &entry.usage;
    println!(
        "## {name} · turno {} ({}) · {} {}/{}",
        entry.turn,
        if entry.resumed { "continúa" } else { "nuevo" },
        role.cli,
        role.model,
        role.effort.as_deref().unwrap_or("-")
    );
    println!(
        "estado: {} · {} s · tokens: {} entrada ({} en caché) · {} salida",
        entry.status, entry.duration_s, u.input_tokens, u.cached_input_tokens, u.output_tokens
    );
    if let Some(commit) = &entry.commit {
        println!(
            "commit: {commit} · {} ficheros: {}",
            entry.changed_files.len(),
            entry.changed_files.join(", ")
        );
    }
    println!(
        "avisos: {}",
        if entry.warnings.is_empty() {
            "ninguno".to_string()
        } else {
            entry.warnings.join("; ")
        }
    );
    println!("salida: {}", entry.output);
    if ctx.config.flow.review_role.as_deref() == Some(name) {
        println!(
            "ronda de revisión: {rounds}/{}",
            ctx.config.flow.max_review_rounds
        );
    }
    match review {
        Some(review) => {
            println!("veredicto: {}", review["verdict"].as_str().unwrap_or("?"));
            for (key, label) in [("summary", "resumen"), ("tests_assessment", "tests")] {
                if let Some(value) = review[key].as_str().filter(|v| !v.is_empty()) {
                    println!("{label}: {value}");
                }
            }
            for f in review["findings"].as_array().into_iter().flatten() {
                let line = match &f["line"] {
                    Value::Number(n) => n.to_string(),
                    _ => "?".into(),
                };
                println!(
                    "- [{}] {}:{line} — {} → {}",
                    f["severity"].as_str().unwrap_or("?"),
                    f["file"].as_str().unwrap_or("?"),
                    f["issue"].as_str().unwrap_or(""),
                    f["suggestion"].as_str().unwrap_or("")
                );
            }
            for q in review["questions_for_director"]
                .as_array()
                .into_iter()
                .flatten()
            {
                println!("- Pregunta: {}", q.as_str().unwrap_or(""));
            }
        }
        None => println!("\n{text}"),
    }
}
