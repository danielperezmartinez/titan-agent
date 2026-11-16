//! `ask`: one turn of a Codex role in the task's worktree, with its events
//! (`brief_sent`, `turn_started`, `activity`, `turn_finished`, `report`).

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use titan_agent_core::team::{EventKind, Findings, TEXT_LIMIT, clip, first_line};

use crate::commands::{run_tests, worktree};
use crate::config::{Role, TeamConfig, resolve};
use crate::process::{git, git_ok, path_warnings};
use crate::state::{self, EventLog, HistoryEntry, RoleState, now_iso, relative, task_dir};
use crate::{Args, Failure, codex};

/// Largest diff added to a prompt by `--diff`, in chars.
const DIFF_LIMIT: usize = 60_000;

fn launchable<'a>(team: &'a TeamConfig, name: &str) -> Result<&'a Role, Failure> {
    let role = team
        .role(name)
        .ok_or_else(|| Failure::usage(format!("Rol inexistente: {name}")))?;
    if !role.launch {
        return Err(Failure::usage(format!(
            "El rol {name} tiene launch = false y no se puede encargar"
        )));
    }
    if role.cli != "codex" {
        return Err(Failure::usage(format!(
            "El rol {name} usa cli = {}; titan-team solo lanza roles de Codex",
            role.cli
        )));
    }
    Ok(role)
}

/// Whether `program` can be started: a path that exists or a name on PATH.
fn available(program: &str) -> bool {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file();
    }
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            dir.join(program).is_file()
                || (cfg!(windows) && dir.join(format!("{program}.exe")).is_file())
        })
    })
}

pub fn ask(team: &TeamConfig, rest: &[String], args: &Args, cwd: &Path) -> Result<i32, Failure> {
    let [role_name, task] = rest else {
        return Err(Failure::usage(
            "Uso: titan-team ask <rol> <encargo> [opciones]",
        ));
    };
    let role = launchable(team, role_name)?;
    let mut state = state::load(team, task)?;
    let wt = worktree(team, &state);
    if !wt.is_dir() {
        return Err(Failure::usage(format!(
            "No existe el worktree: {}",
            wt.display()
        )));
    }
    let known = state.roles.get(role_name);
    let thread_id = if args.flag("--new") {
        None
    } else {
        known.and_then(|r| r.thread_id.clone())
    };
    let message = match (
        args.option(&["-m", "--message"]),
        args.option(&["-f", "--file"]),
    ) {
        (Some(_), Some(_)) => return Err(Failure::usage("Usa -m o -f, no los dos")),
        (Some(text), None) => Some(text.to_string()),
        (None, Some(file)) => Some(
            std::fs::read_to_string(resolve(cwd, Path::new(file)))
                .map_err(|e| Failure::usage(format!("No se pudo leer el mensaje: {e}")))?,
        ),
        (None, None) => None,
    };
    if thread_id.is_some() && message.is_none() {
        return Err(Failure::usage(
            "Para continuar un hilo es obligatorio indicar -m o -f",
        ));
    }

    let mut prompt = match &thread_id {
        Some(_) => format!(
            "# Mensaje del director\n\n{}",
            message.clone().unwrap_or_default()
        ),
        None => {
            let prompt_file = role.prompt_file.as_deref().unwrap_or(Path::new(""));
            let read = |path: &Path| {
                std::fs::read_to_string(path).map_err(|e| {
                    Failure::usage(format!("No se pudo leer el prompt o encargo: {e}"))
                })
            };
            let mut text = format!(
                "{}\n\n# Encargo\n\n{}",
                read(prompt_file)?,
                read(&team.path(Path::new(&state.brief)))?
            );
            if let Some(message) = &message {
                text.push_str(&format!("\n\n# Mensaje del director\n\n{message}"));
            }
            text
        }
    };
    let mut warnings = Vec::new();
    if args.flag("--diff") {
        let mut diff = git(&wt, &["diff", &format!("{}..HEAD", state.base_commit)])?;
        if diff.chars().count() > DIFF_LIMIT {
            diff = diff.chars().take(DIFF_LIMIT).collect();
            warnings.push("Diff truncado a 60 000 caracteres".to_string());
        }
        prompt.push_str(&format!("\n\n# Diff respecto a la base\n\n{diff}"));
    }
    if args.flag("--test") {
        let (code, output) = run_tests(team, &state)?;
        prompt.push_str(&format!(
            "\n\n# Resultado de los tests (exit {code})\n\n{output}"
        ));
    }

    let is_reviewer = team.flow.review_role.as_deref() == Some(role_name.as_str());
    let round = state.review_rounds + u32::from(is_reviewer);
    if is_reviewer && round > team.flow.max_review_rounds && !args.flag("--force") {
        return Err(Failure {
            code: 3,
            message: format!(
                "Se superó el máximo de rondas de revisión ({}); usa --force",
                team.flow.max_review_rounds
            ),
        });
    }
    let n = state.history.len() as u32 + 1;
    let dir = task_dir(team, task);
    let file = |suffix: &str| dir.join(format!("{n:02}-{}.{suffix}", safe(role_name)));
    let response = file("response");
    let program = codex::executable();
    let argv = codex::argv(team, role, thread_id.as_deref(), &response, &wt);
    if args.flag("--dry-run") {
        let mut line = vec![program.clone()];
        line.extend(argv.iter().cloned());
        println!("Comando: {}\n\n--- prompt ---\n{prompt}", shell_join(&line));
        return Ok(0);
    }
    if !available(&program) {
        return Err(Failure::usage(format!(
            "{program} no está disponible en PATH"
        )));
    }
    std::fs::create_dir_all(&dir).map_err(Failure::io)?;

    let turn = known.map_or(0, |r| r.turns) + 1;
    let log = EventLog::new(team, task);
    let director = team.director().map(str::to_string);
    log.append(EventKind::BriefSent {
        from: director.clone(),
        to: role_name.clone(),
        turn,
        resumed: thread_id.is_some(),
        text: clip(
            message
                .as_deref()
                .and_then(first_line)
                .unwrap_or("encargo inicial"),
            TEXT_LIMIT,
        ),
    });
    log.append(EventKind::TurnStarted {
        role: role_name.clone(),
        turn,
        cli: role.cli.clone(),
        model: role.model.clone(),
        effort: role.effort.clone(),
        sandbox: Some(role.sandbox.clone()),
        thread_id: thread_id.clone(),
        review_round: is_reviewer.then_some(round),
    });
    let started = Instant::now();
    let outcome = codex::run(
        &program,
        &argv,
        &prompt,
        &wt,
        Duration::from_secs(role.timeout_s),
        |text| {
            log.append(EventKind::Activity {
                role: role_name.clone(),
                turn,
                text,
            })
        },
    );
    let out = match outcome {
        Ok(out) => out,
        Err(error) => codex::TurnOutput {
            status: "error",
            stderr: error.clone(),
            errors: vec![error],
            ..Default::default()
        },
    };
    let duration = started.elapsed();
    let _ = std::fs::write(file("jsonl"), &out.stdout);
    let _ = std::fs::write(file("stderr"), &out.stderr);
    let text = std::fs::read_to_string(&response).unwrap_or_else(|_| out.last_message.clone());
    let _ = std::fs::remove_file(&response);
    let report: Option<Value> = role
        .output_schema
        .as_ref()
        .and_then(|_| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object);
    let output_file = file(if role.output_schema.is_some() {
        "json"
    } else {
        "md"
    });
    std::fs::write(&output_file, &text).map_err(Failure::io)?;

    let (commit, changed) = commit_turn(team, role, &wt, role_name, turn, &mut warnings)?;
    warnings.extend(path_warnings(
        &role.allowed_paths,
        &role.forbidden_paths,
        &changed,
    ));
    let thread = out.thread_id.clone().or(thread_id.clone());
    let findings = report.as_ref().map(count_findings);
    let verdict = report
        .as_ref()
        .and_then(|r| r.get("verdict"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let summary = match (&verdict, findings) {
        (Some(verdict), Some(findings)) => format!("{verdict} · {} hallazgos", findings.total()),
        _ => first_line(&text).unwrap_or(out.status).to_string(),
    };
    log.append(EventKind::TurnFinished {
        role: role_name.clone(),
        turn,
        status: out.status.to_string(),
        duration_ms: duration.as_millis() as u64,
        thread_id: thread.clone(),
        usage: out.usage,
        commit: commit.clone(),
        changed_files: changed.clone(),
        warnings: warnings.clone(),
        text: clip(&summary, TEXT_LIMIT),
    });
    if let (Some(verdict), Some(findings)) = (&verdict, findings) {
        let questions = report
            .as_ref()
            .and_then(|r| r.get("questions_for_director"))
            .and_then(Value::as_array)
            .map_or(0, |q| q.len() as u32);
        log.append(EventKind::Report {
            role: role_name.clone(),
            review_round: is_reviewer.then_some(round),
            verdict: verdict.clone(),
            findings,
            questions,
        });
    }

    let entry = HistoryEntry {
        n,
        ts: now_iso(),
        role: role_name.clone(),
        turn,
        thread_id: thread.clone(),
        resumed: thread_id.is_some(),
        status: out.status.to_string(),
        duration_s: (duration.as_secs_f64() * 10.0).round() / 10.0,
        usage: out.usage,
        output: relative(&team.base, &output_file),
        commit,
        changed_files: changed,
        warnings,
    };
    state.history.push(entry.clone());
    state.roles.insert(
        role_name.clone(),
        RoleState {
            cli: role.cli.clone(),
            model: role.model.clone(),
            thread_id: thread,
            turns: turn,
        },
    );
    if is_reviewer {
        state.review_rounds = round;
    }
    state::save(team, &state)?;

    if args.flag("--json") {
        let mut value = serde_json::to_value(&entry).unwrap_or(Value::Null);
        value["text"] = json!(text);
        value["parsed"] = report.clone().unwrap_or(Value::Null);
        println!("{value}");
    } else {
        print_report(
            team,
            role,
            &entry,
            &text,
            report.as_ref(),
            is_reviewer.then_some(round),
        );
    }
    Ok(if out.status == "ok" { 0 } else { 1 })
}

fn safe(role: &str) -> String {
    role.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn shell_join(words: &[String]) -> String {
    words
        .iter()
        .map(|w| {
            if !w.is_empty()
                && w.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_./=:@".contains(c))
            {
                w.clone()
            } else {
                format!("'{}'", w.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn count_findings(report: &Value) -> Findings {
    let mut findings = Findings::default();
    for finding in report
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match finding.get("severity").and_then(Value::as_str) {
            Some("blocker") => findings.blocker += 1,
            Some("major") => findings.major += 1,
            Some("minor") => findings.minor += 1,
            _ => findings.nit += 1,
        }
    }
    findings
}

/// Commits what a writing role changed (with `project.autocommit`), or
/// warns when a read-only role left changes.
fn commit_turn(
    team: &TeamConfig,
    role: &Role,
    wt: &Path,
    name: &str,
    turn: u32,
    warnings: &mut Vec<String>,
) -> Result<(Option<String>, Vec<String>), Failure> {
    if role.sandbox == "read-only" {
        if !git(wt, &["status", "--porcelain"])?.trim().is_empty() {
            warnings.push("El rol read-only dejó cambios en el árbol".into());
        }
        return Ok((None, Vec::new()));
    }
    if !team.project.autocommit {
        return Ok((None, Vec::new()));
    }
    git(wt, &["add", "-A"])?;
    if git_ok(wt, &["diff", "--cached", "--quiet"]) {
        return Ok((None, Vec::new()));
    }
    let identity = git_ok(wt, &["config", "user.name"]) && git_ok(wt, &["config", "user.email"]);
    let message = format!("team: {name} turno {turn}");
    let mut commit_args = Vec::new();
    if !identity {
        commit_args.extend(["-c", "user.name=team", "-c", "user.email=team@localhost"]);
    }
    commit_args.extend(["commit", "-m", &message]);
    git(wt, &commit_args)?;
    let hash = git(wt, &["rev-parse", "--short", "HEAD"])?
        .trim()
        .to_string();
    let changed = git(wt, &["diff", "--name-only", "HEAD~1..HEAD"])?
        .lines()
        .map(str::to_string)
        .collect();
    Ok((Some(hash), changed))
}

fn print_report(
    team: &TeamConfig,
    role: &Role,
    entry: &HistoryEntry,
    text: &str,
    report: Option<&Value>,
    round: Option<u32>,
) {
    let u = &entry.usage;
    println!(
        "## {} · turno {} ({}) · {} {}/{}",
        entry.role,
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
    if let Some(round) = round {
        println!("ronda de revisión: {round}/{}", team.flow.max_review_rounds);
    }
    match report.filter(|r| r.get("verdict").is_some()) {
        Some(review) => {
            let field = |key: &str| review.get(key).and_then(Value::as_str).unwrap_or("");
            println!("veredicto: {}", field("verdict"));
            for (key, label) in [("summary", "resumen"), ("tests_assessment", "tests")] {
                if !field(key).is_empty() {
                    println!("{label}: {}", field(key));
                }
            }
            for f in review
                .get("findings")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let get = |key: &str| match f.get(key) {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Null) | None => "?".into(),
                    Some(other) => other.to_string(),
                };
                println!(
                    "- [{}] {}:{} — {} → {}",
                    get("severity"),
                    get("file"),
                    get("line"),
                    get("issue"),
                    get("suggestion")
                );
            }
            for q in review
                .get("questions_for_director")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                println!("- Pregunta: {}", q.as_str().unwrap_or_default());
            }
        }
        None => println!("\n{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findings_are_counted_by_severity() {
        let report = json!({"verdict":"changes_requested","findings":[
            {"severity":"major"},{"severity":"major"},{"severity":"nit"},{"severity":"odd"}]});
        let findings = count_findings(&report);
        assert_eq!((findings.major, findings.nit, findings.total()), (2, 2, 4));
    }

    #[test]
    fn dry_run_lines_quote_what_needs_it() {
        let words = ["codex", "-c", "sandbox_mode=\"read-only\"", "a b", "-"].map(String::from);
        assert_eq!(
            shell_join(&words),
            "codex -c 'sandbox_mode=\"read-only\"' 'a b' -"
        );
        assert_eq!(safe("rev/isor"), "rev_isor");
    }
}
