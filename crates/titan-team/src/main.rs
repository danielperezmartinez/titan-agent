//! titan-team: the team tool of titan-agent (docs/Decisiones/ADR-0005 and
//! ADR-0010). The director (a Claude Code session) runs it through Bash to
//! hand work to the Codex roles of `.agents/team.toml`; every step is
//! written to a versioned events log that titan-agentd reads for the monitor.
//!
//! It only launches `codex exec` (never `claude`), never with
//! `danger-full-access`, and reads no credentials of any CLI.

mod ask;
mod checks;
mod codex;
mod commands;
mod config;
mod process;
mod state;

use std::path::PathBuf;

/// An error with the exit code it ends the program with: 1 a failed turn or
/// test, 2 usage, configuration or state, 3 the review round limit.
#[derive(Debug)]
pub struct Failure {
    pub code: i32,
    pub message: String,
}

impl Failure {
    pub fn usage(message: impl Into<String>) -> Failure {
        Failure {
            code: 2,
            message: message.into(),
        }
    }

    pub fn io(error: std::io::Error) -> Failure {
        Failure::usage(error.to_string())
    }
}

const HELP: &str = "titan-team: encargos del equipo definido en .agents/team.toml

Uso: titan-team [--config RUTA] <orden> [opciones]

Órdenes:
  init [--force]                         crea .agents/team.toml de partida aquí
  doc                                    formato de team.toml, salidas y eventos
  check [--json]                         valida team.toml
  roles [--json]                         lista los roles y el flujo
  new <encargo> --brief FICHERO [--base REV]
  ask <rol> <encargo> [-m TEXTO | -f FICHERO] [--diff] [--test] [--gate]
                      [--allow PATRÓN]... [--new] [--force] [--dry-run] [--json]
  show <encargo> [--json]
  diff <encargo> [--stat]
  test <encargo>
  decide <encargo> -m TEXTO
  close <encargo> [--delete-branch]

Salida: 0 bien · 1 turno o test fallido · 2 uso, configuración o estado · 3 tope de rondas
        · 4 hilo del rol abierto en otro sitio (ciérralo y repite; el hilo se conserva).
Formato de team.toml y de los eventos: titan-team doc.";

/// Parsed command line: positional words, flags and `-x VALUE` options.
#[derive(Debug)]
pub struct Args {
    pub words: Vec<String>,
    flags: Vec<String>,
    options: Vec<(String, String)>,
}

impl Args {
    pub fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }

    pub fn option(&self, names: &[&str]) -> Option<&str> {
        self.options
            .iter()
            .rev()
            .find(|(key, _)| names.contains(&key.as_str()))
            .map(|(_, value)| value.as_str())
    }

    /// Every value of a repeatable option, in order.
    pub fn options(&self, name: &str) -> Vec<&str> {
        self.options
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }
}

const VALUE_OPTIONS: [&str; 8] = [
    "--config",
    "--allow",
    "--brief",
    "--base",
    "-m",
    "--message",
    "-f",
    "--file",
];
const FLAGS: [&str; 10] = [
    "--json",
    "--diff",
    "--test",
    "--gate",
    "--new",
    "--force",
    "--dry-run",
    "--stat",
    "--delete-branch",
    "--help",
];

/// What each command accepts: its positional arguments and its options
/// (`--config` and `--help` go with every command).
fn accepted(command: &str) -> Option<(usize, &'static [&'static str])> {
    Some(match command {
        "init" => (0, &["--force"]),
        "doc" => (0, &[]),
        "check" | "roles" => (0, &["--json"]),
        "new" => (1, &["--brief", "--base"]),
        "ask" => (
            2,
            &[
                "-m",
                "--message",
                "-f",
                "--file",
                "--diff",
                "--test",
                "--gate",
                "--allow",
                "--new",
                "--force",
                "--dry-run",
                "--json",
            ],
        ),
        "show" => (1, &["--json"]),
        "diff" => (1, &["--stat"]),
        "test" => (1, &[]),
        "decide" => (1, &["-m", "--message"]),
        "close" => (1, &["--delete-branch"]),
        _ => return None,
    })
}

/// Refuses options and arguments the command does not take, before it does
/// anything (an ignored `--dry-run` must not let `close` run for real).
fn check_args(args: &Args) -> Result<(), Failure> {
    let command = args.words[0].as_str();
    let Some((count, options)) = accepted(command) else {
        return Err(Failure::usage(format!(
            "orden desconocida: {command} (titan-team --help)"
        )));
    };
    let given = args
        .flags
        .iter()
        .chain(args.options.iter().map(|(key, _)| key))
        .filter(|key| !matches!(key.as_str(), "--config" | "--help"));
    for key in given {
        if !options.contains(&key.as_str()) {
            return Err(Failure::usage(format!("{command} no admite {key}")));
        }
    }
    let positional = args.words.len() - 1;
    if positional != count {
        return Err(Failure::usage(format!(
            "{command} lleva {count} argumento(s) y se dieron {positional} (titan-team --help)"
        )));
    }
    Ok(())
}

fn parse_args(raw: Vec<String>) -> Result<Args, Failure> {
    let mut args = Args {
        words: Vec::new(),
        flags: Vec::new(),
        options: Vec::new(),
    };
    let mut iter = raw.into_iter();
    while let Some(arg) = iter.next() {
        if VALUE_OPTIONS.contains(&arg.as_str()) {
            let value = iter
                .next()
                .ok_or_else(|| Failure::usage(format!("{arg} necesita un valor")))?;
            // A pattern is never an option: `--allow --dry-run` must not
            // swallow the dry run (use `--allow=-x` for a pattern like that).
            if arg == "--allow"
                && (FLAGS.contains(&value.as_str()) || VALUE_OPTIONS.contains(&value.as_str()))
            {
                return Err(Failure::usage("--allow necesita un valor"));
            }
            args.options.push((arg, value));
        } else if let Some((key, value)) = arg.split_once('=')
            && VALUE_OPTIONS.contains(&key)
            && key.starts_with("--")
        {
            args.options.push((key.to_string(), value.to_string()));
        } else if FLAGS.contains(&arg.as_str()) || arg == "-h" {
            args.flags
                .push(if arg == "-h" { "--help".into() } else { arg });
        } else if arg.starts_with('-') && arg != "-" {
            return Err(Failure::usage(format!("opción desconocida: {arg}")));
        } else {
            args.words.push(arg);
        }
    }
    Ok(args)
}

fn run(raw: Vec<String>) -> Result<i32, Failure> {
    let args = parse_args(raw)?;
    if args.flag("--help") || args.words.is_empty() {
        println!("{HELP}");
        return Ok(if args.words.is_empty() && !args.flag("--help") {
            2
        } else {
            0
        });
    }
    check_args(&args)?;
    let cwd: PathBuf = std::env::current_dir().map_err(Failure::io)?;
    // These two work without a team.toml: they are how a project gets one.
    match args.words[0].as_str() {
        "doc" => {
            print!("{}", include_str!("../FORMAT.md"));
            return Ok(0);
        }
        "init" => return commands::init(&cwd, &args),
        _ => {}
    }
    let path = config::find(args.option(&["--config"]), &cwd).map_err(Failure::usage)?;
    let command = args.words[0].as_str();
    let team = match config::load(&path) {
        Ok(team) => team,
        Err(errors) => return Ok(commands::report_invalid(command, &args, &errors)),
    };
    let rest = &args.words[1..];
    match command {
        "check" => commands::check(&team, &args),
        "roles" => commands::roles(&team, &args),
        "new" => commands::new(&team, rest, &args, &cwd),
        "ask" => ask::ask(&team, rest, &args, &cwd),
        "show" => commands::show(&team, rest, &args),
        "diff" => commands::diff(&team, rest, &args),
        "test" => commands::test(&team, rest),
        "decide" => commands::decide(&team, rest, &args),
        "close" => commands::close(&team, rest, &args),
        other => Err(Failure::usage(format!(
            "orden desconocida: {other} (titan-team --help)"
        ))),
    }
}

fn main() {
    let code = match run(std::env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(failure) => {
            eprintln!("{}", failure.message);
            failure.code
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_flags_and_words_are_told_apart() {
        let args = parse_args(
            [
                "ask",
                "rol",
                "x",
                "-m",
                "--hola",
                "--json",
                "--base=dev",
                "-",
            ]
            .map(String::from)
            .to_vec(),
        )
        .unwrap();
        assert_eq!(args.words, ["ask", "rol", "x", "-"]);
        assert_eq!(args.option(&["-m", "--message"]), Some("--hola"));
        assert_eq!(args.option(&["--base"]), Some("dev"));
        assert!(args.flag("--json"));
        assert_eq!(parse_args(vec!["--nope".into()]).unwrap_err().code, 2);
        assert_eq!(parse_args(vec!["-m".into()]).unwrap_err().code, 2);
        let words = |line: &str| parse_args(line.split(' ').map(String::from).collect()).unwrap();
        assert!(check_args(&words("close demo --delete-branch")).is_ok());
        assert_eq!(
            check_args(&words("close demo --dry-run")).unwrap_err().code,
            2
        );
        assert_eq!(check_args(&words("show demo extra")).unwrap_err().code, 2);
        assert_eq!(check_args(&words("ask r")).unwrap_err().code, 2);
        assert!(check_args(&words("ask r t -m hi --json --config x")).is_ok());
        let allow = words("ask r t --allow a/ --gate --allow=b.rs");
        assert!(check_args(&allow).is_ok());
        assert_eq!(allow.options("--allow"), ["a/", "b.rs"]);
        assert_eq!(parse_args(vec!["--allow".into()]).unwrap_err().code, 2);
        let swallowed = parse_args(
            ["ask", "r", "t", "--allow", "--dry-run"]
                .map(String::from)
                .to_vec(),
        );
        assert_eq!(swallowed.unwrap_err().code, 2);
        assert!(parse_args(vec!["--allow=--odd".into()]).is_ok());
        assert_eq!(check_args(&words("show t --gate")).unwrap_err().code, 2);
        assert_eq!(check_args(&words("fly")).unwrap_err().code, 2);
    }
}
