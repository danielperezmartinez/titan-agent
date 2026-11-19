//! `ask`: one turn of a Codex role in the task's worktree, with its events
//! (`brief_sent`, `turn_started`, `activity`, `turn_finished`, `report`).

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use titan_agent_core::team::{EventKind, Findings, STATUS_BUSY, TEXT_LIMIT, clip, first_line};

use crate::checks::{self, CheckRun};
use crate::commands::{run_tests, worktree};
use crate::config::{Role, TeamConfig, resolve};
use crate::process::{git, git_ok, last_lines, path_matches, path_warnings};
use crate::state::{self, EventLog, HistoryEntry, RoleState, now_iso, relative, task_dir};
use crate::{Args, Failure, codex};

/// Exit code of a turn refused because the role's thread is open elsewhere.
pub const BUSY_EXIT: i32 = 4;

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

/// The sections of `required` that no Markdown heading of `brief` names:
/// an ATX heading (up to 3 spaces, 1–6 `#` and a space) whose text
/// contains the section, ignoring case. Lines inside a fenced code block
/// are not headings.
pub fn missing_sections<'a>(brief: &str, required: &'a [String]) -> Vec<&'a str> {
    let mut headings = Vec::new();
    let mut fenced = false;
    for line in brief.lines() {
        let indent = line.len() - line.trim_start_matches(' ').len();
        let rest = &line[indent..];
        if indent <= 3 && rest.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced || indent > 3 {
            continue;
        }
        let hashes = rest.len() - rest.trim_start_matches('#').len();
        if (1..=6).contains(&hashes) && rest[hashes..].starts_with(' ') {
            headings.push(rest[hashes..].trim().to_lowercase());
        }
    }
    required
        .iter()
        .filter(|section| {
            let section = section.trim().to_lowercase();
            !headings.iter().any(|heading| heading.contains(&section))
        })
        .map(String::as_str)
        .collect()
}

pub fn ask(team: &TeamConfig, rest: &[String], args: &Args, cwd: &Path) -> Result<i32, Failure> {
    let [role_name, task] = rest else {
        return Err(Failure::usage(
            "Uso: titan-team ask <rol> <encargo> [opciones]",
        ));
    };
    let role = launchable(team, role_name)?;
    // One command at a time per task: two turns must not share a history
    // number, a review round or a thread.
    let _lock = state::lock_task(team, task)?;
    let mut state = state::load(team, task)?;
    let wt = worktree(team, &state);
    if !wt.is_dir() {
        return Err(Failure::usage(format!(
            "No existe el worktree: {}",
            wt.display()
        )));
    }
    let known = state.roles.get(role_name);
    let previous_thread = known.and_then(|r| r.thread_id.clone());
    let thread_id = if args.flag("--new") {
        None
    } else {
        known.and_then(|r| r.thread_id.clone())
    };
    let message = match (
        args.option(&["-m", "--message"]),
        args.option(&["-f", "--file", "--message-file"]),
    ) {
        (Some(_), Some(_)) => {
            return Err(Failure::usage("Usa -m o -f/--message-file, no los dos"));
        }
        (Some(text), None) => Some(text.to_string()),
        (None, Some(file)) => Some(
            std::fs::read_to_string(resolve(cwd, Path::new(file)))
                .map_err(|e| Failure::usage(format!("No se pudo leer el mensaje: {e}")))?,
        ),
        (None, None) => None,
    };
    // An empty message is a mistake upstream (a file that was never
    // written, an empty `$(cat …)`): never hand it to a role as if it
    // were the director's word.
    if message
        .as_deref()
        .is_some_and(|text| text.trim().is_empty())
    {
        return Err(Failure::usage(
            "El mensaje está vacío (-m o -f/--message-file): no se lanza el turno",
        ));
    }
    if thread_id.is_some() && message.is_none() {
        return Err(Failure::usage(
            "Para continuar un hilo es obligatorio indicar -m o -f",
        ));
    }
    if thread_id.is_none() && !role.brief_sections.is_empty() {
        let brief = std::fs::read_to_string(team.path(Path::new(&state.brief)))
            .map_err(|e| Failure::usage(format!("No se pudo leer el encargo: {e}")))?;
        let missing = missing_sections(&brief, &role.brief_sections);
        if !missing.is_empty() {
            return Err(Failure::usage(format!(
                "Al encargo le falta {} para {role_name} (brief_sections): {}; \
                 añádela como título Markdown y vuelve a crearlo con new --brief",
                if missing.len() == 1 {
                    "una sección"
                } else {
                    "secciones"
                },
                missing.join(", ")
            )));
        }
    }
    if args.flag("--gate") && team.project.gate_cmd.is_none() {
        return Err(Failure::usage(
            "--gate necesita project.gate_cmd en team.toml",
        ));
    }
    let allowed: Vec<String> = args
        .options("--allow")
        .into_iter()
        .map(str::to_string)
        .collect();

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
        if args.flag("--dry-run") {
            prompt.push_str("\n\n# Resultado de los tests\n\n(no se ejecutan en --dry-run)");
        } else {
            let (code, output) = run_tests(team, &state)?;
            prompt.push_str(&format!(
                "\n\n# Resultado de los tests (exit {code})\n\n{}",
                last_lines(&output, 150)
            ));
        }
    }
    if args.flag("--gate") && args.flag("--dry-run") {
        prompt.push_str("\n\n# Compuerta\n\n(no se ejecuta en --dry-run)");
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
    })?;
    log.append(EventKind::TurnStarted {
        role: role_name.clone(),
        turn,
        cli: role.cli.clone(),
        model: role.model.clone(),
        effort: role.effort.clone(),
        sandbox: Some(role.sandbox.clone()),
        thread_id: thread_id.clone(),
        review_round: is_reviewer.then_some(round),
    })?;
    // The gate runs once the turn is on the log, so the monitor shows what
    // the role is waiting for (a whole build can take minutes). Nothing here
    // may end the command: the turn is already open in the log.
    if let Some(cmd) = team
        .project
        .gate_cmd
        .as_deref()
        .filter(|_| args.flag("--gate"))
    {
        if let Err(error) = log.append(EventKind::Activity {
            role: role_name.clone(),
            turn,
            text: clip(&format!("$ {cmd}"), TEXT_LIMIT),
        }) {
            eprintln!("aviso: {}", error.message);
        }
        let block = checks::gate(team, cmd, &wt);
        if let Err(error) = std::fs::write(file("gate.txt"), &block) {
            eprintln!("aviso: no se pudo guardar la compuerta: {error}");
        }
        prompt.push_str(&format!("\n\n{block}"));
    }
    let started = Instant::now();
    // Only a writing role builds during its turn (the read-only sandbox
    // could not write the shared folder anyway).
    let build_env = if role.sandbox == "workspace-write" {
        team.project.build_env()
    } else {
        Vec::new()
    };
    for (_, dir) in &build_env {
        if let Err(error) = std::fs::create_dir_all(dir) {
            eprintln!("aviso: no se pudo crear {}: {error}", dir.display());
        }
    }
    let outcome = codex::run(
        &program,
        &argv,
        &prompt,
        &wt,
        Duration::from_secs(role.timeout_s),
        &build_env,
        |text| {
            // Activity is best effort: losing a line must not stop the turn.
            if let Err(error) = log.append(EventKind::Activity {
                role: role_name.clone(),
                turn,
                text,
            }) {
                eprintln!("aviso: {}", error.message);
            }
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
    // The thread is open elsewhere (another writer): nothing ran. The role
    // keeps its thread and its turn count, and no new thread is opened.
    let busy = out.status == STATUS_BUSY;
    // From here on the turn has run: every step is recorded even if an
    // earlier one fails, and the failures are reported at the end.
    let mut failures: Vec<String> = Vec::new();
    for (suffix, content) in [("jsonl", &out.stdout), ("stderr", &out.stderr)] {
        if let Err(error) = std::fs::write(file(suffix), content) {
            failures.push(format!(
                "No se pudo guardar {}: {error}",
                file(suffix).display()
            ));
        }
    }
    // The role's checks run before the commit, so what they change (the
    // formatting) goes in the turn's commit.
    let mut status = out.status;
    let mut check_runs = Vec::new();
    if status == "ok" && !role.checks.is_empty() {
        let (runs, report) = checks::run_checks(team, role, &wt, |cmd| {
            if let Err(error) = log.append(EventKind::Activity {
                role: role_name.clone(),
                turn,
                text: clip(&format!("$ {cmd}"), TEXT_LIMIT),
            }) {
                eprintln!("aviso: {}", error.message);
            }
        });
        if let Err(error) = std::fs::write(file("checks.txt"), report) {
            failures.push(format!(
                "No se pudo guardar {}: {error}",
                file("checks.txt").display()
            ));
        }
        warnings.extend(runs.iter().filter(|r| !r.passed()).map(CheckRun::warning));
        if runs
            .iter()
            .any(|r| !r.passed() && role.checks_required.contains(&r.name))
        {
            status = "error";
        }
        check_runs = runs;
    }
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
    if let Err(error) = std::fs::write(&output_file, &text) {
        failures.push(format!(
            "No se pudo guardar {}: {error}",
            output_file.display()
        ));
    }

    let committed = if busy {
        Ok((None, Vec::new()))
    } else {
        commit_turn(team, role, &wt, role_name, turn, &mut warnings)
    };
    let (commit, changed) = match committed {
        Ok(done) => done,
        Err(error) => {
            warnings.push(format!(
                "No se pudo hacer el commit del turno: {}",
                error.message
            ));
            failures.push(format!(
                "No se pudo hacer el commit del turno: {}",
                error.message
            ));
            (None, Vec::new())
        }
    };
    // Files the director authorized for this turn (`--allow`) warn of nothing.
    let watched: Vec<String> = changed
        .iter()
        .filter(|file| !allowed.iter().any(|pattern| path_matches(file, pattern)))
        .cloned()
        .collect();
    warnings.extend(path_warnings(
        &role.allowed_paths,
        &role.forbidden_paths,
        &watched,
    ));
    let thread = if busy {
        thread_id.clone().or(previous_thread)
    } else {
        out.thread_id.clone().or(thread_id.clone())
    };
    let findings = report.as_ref().map(count_findings);
    let verdict = report
        .as_ref()
        .and_then(|r| r.get("verdict"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let summary = match (&verdict, findings) {
        _ if busy => format!("hilo de {role_name} abierto en titan-agent"),
        (Some(verdict), Some(findings)) => match findings.total() {
            1 => format!("{verdict} · 1 hallazgo"),
            n => format!("{verdict} · {n} hallazgos"),
        },
        _ => first_line(&text).unwrap_or(status).to_string(),
    };
    log.append(EventKind::TurnFinished {
        role: role_name.clone(),
        turn,
        status: status.to_string(),
        duration_ms: duration.as_millis() as u64,
        thread_id: thread.clone(),
        usage: out.usage,
        commit: commit.clone(),
        changed_files: changed.clone(),
        warnings: warnings.clone(),
        text: clip(&summary, TEXT_LIMIT),
    })
    .unwrap_or_else(|error| failures.push(error.message));
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
        })
        .unwrap_or_else(|error| failures.push(error.message));
    }

    let entry = HistoryEntry {
        n,
        ts: now_iso(),
        role: role_name.clone(),
        turn,
        thread_id: thread.clone(),
        turn_id: None,
        resumed: thread_id.is_some(),
        status: status.to_string(),
        duration_s: (duration.as_secs_f64() * 10.0).round() / 10.0,
        usage: out.usage,
        output: relative(&team.base, &output_file),
        commit,
        changed_files: changed,
        warnings,
        checks: check_runs,
        allowed,
    };
    state.history.push(entry.clone());
    state.roles.insert(
        role_name.clone(),
        RoleState {
            cli: role.cli.clone(),
            model: role.model.clone(),
            thread_id: thread.clone(),
            turns: if busy { turn - 1 } else { turn },
        },
    );
    if is_reviewer && !busy {
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
            (is_reviewer && !busy).then_some(round),
        );
    }
    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("{failure}");
        }
        if !busy {
            return Ok(2);
        }
    }
    if busy {
        eprintln!(
            "El hilo de {role_name} está abierto en titan-agent; ciérralo o espera y repite la orden.{}",
            thread
                .as_deref()
                .map(|t| format!(" Su hilo se conserva ({t})."))
                .unwrap_or_default()
        );
        return Ok(BUSY_EXIT);
    }
    Ok(if status == "ok" { 0 } else { 1 })
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
        // No commit, but the changed files still get their warnings.
        let status = git(wt, &["status", "--porcelain", "--untracked-files=all"])?;
        let changed = status
            .lines()
            .filter_map(|line| line.get(3..))
            .map(|path| {
                path.rsplit(" -> ")
                    .next()
                    .unwrap_or(path)
                    .trim_matches('"')
                    .to_string()
            })
            .collect();
        return Ok((None, changed));
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
    if !entry.checks.is_empty() {
        let stem = entry
            .output
            .rsplit_once('.')
            .map_or(entry.output.as_str(), |(stem, _)| stem);
        println!(
            "comprobaciones: {} → {stem}.checks.txt",
            checks::summary(&entry.checks)
        );
    }
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

    #[test]
    fn brief_sections_are_found_in_headings_only() {
        let required = ["Contrato", "Firmas", "Criterios visuales", "Extra"].map(String::from);
        let brief = "# Encargo\n\nEl Contrato y las Firmas van abajo.\n\
                     ## C. contrato numerado\n   ### FIRMAS que usan los tests\n\
                     ```md\n## Extra\n```\n    ## Extra indentado\n#Extra sin espacio\n\
                     ####### Extra con siete\n## Criterios visuales\n";
        assert_eq!(missing_sections(brief, &required), ["Extra"]);
        assert!(missing_sections(brief, &required[..3]).is_empty());
        assert_eq!(missing_sections("", &required[..1]), ["Contrato"]);
    }
}
