//! titan-team: the team tool of titan-agent (docs/Decisiones/ADR-0005 and
//! ADR-0010). The director (a Claude Code session) runs it through Bash to
//! hand work to the Codex roles of `.agents/team.toml`; every step is
//! written to a versioned events log that titan-agentd reads for the monitor.
//!
//! It only launches `codex exec` (never `claude`), never with
//! `danger-full-access`, and reads no credentials of any CLI.

mod ask;
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
  check [--json]                         valida team.toml
  roles [--json]                         lista los roles y el flujo
  new <encargo> --brief FICHERO [--base REV]
  ask <rol> <encargo> [-m TEXTO | -f FICHERO] [--diff] [--test] [--new]
                      [--force] [--dry-run] [--json]
  show <encargo> [--json]
  diff <encargo> [--stat]
  test <encargo>
  decide <encargo> -m TEXTO
  close <encargo> [--delete-branch]

Salida: 0 bien · 1 turno o test fallido · 2 uso, configuración o estado · 3 tope de rondas.
Formato de team.toml y de los eventos: docs/Decisiones/ADR-0010.";

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
}

const VALUE_OPTIONS: [&str; 7] = [
    "--config",
    "--brief",
    "--base",
    "-m",
    "--message",
    "-f",
    "--file",
];
const FLAGS: [&str; 9] = [
    "--json",
    "--diff",
    "--test",
    "--new",
    "--force",
    "--dry-run",
    "--stat",
    "--delete-branch",
    "--help",
];

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
    let cwd: PathBuf = std::env::current_dir().map_err(Failure::io)?;
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
    }
}
