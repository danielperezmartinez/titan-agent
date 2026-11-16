//! Command line of titan-team (same commands and flags as the prototype,
//! plus `check`, `init`, `doc` and `note`).

use crate::{Result, TeamError};

pub const USAGE: &str = "\
titan-team: encargos del equipo definido en .agents/team.toml

Uso: titan-team [--config RUTA] <orden> [opciones]

Órdenes:
  roles [--json]                    Roles, flujo y tope de rondas
  check                             Valida team.toml y los ficheros que nombra
  init [--force]                    Crea un .agents/team.toml de partida aquí
  doc                               Formato documentado de team.toml y de los eventos
  new <encargo> --brief F [--base B]
                                    Crea el encargo: worktree, rama y estado
  ask <rol> <encargo> [-m TEXTO | -f FICHERO] [--diff] [--test] [--new]
      [--force] [--dry-run] [--json]
                                    Un turno del rol (continúa su hilo si existe)
  show <encargo> [--json]           Estado, hilos y turnos del encargo
  diff <encargo> [--stat]           Diff del worktree respecto a la base
  test <encargo>                    Ejecuta project.test_cmd en el worktree
  note <encargo> -m TEXTO           Anota una decisión del director (sale en el monitor)
  close <encargo> [--delete-branch] Quita el worktree y cierra el encargo

Códigos de salida: 0 bien, 1 el turno falló, 2 error de uso o de configuración,
3 se superó el tope de rondas de revisión (repite con --force si el usuario lo autoriza).
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    Doc,
    Init {
        force: bool,
    },
    Check,
    Roles {
        json: bool,
    },
    New {
        task: String,
        brief: String,
        base: Option<String>,
    },
    Ask(AskArgs),
    Show {
        task: String,
        json: bool,
    },
    Diff {
        task: String,
        stat: bool,
    },
    Test {
        task: String,
    },
    Note {
        task: String,
        text: String,
    },
    Close {
        task: String,
        delete_branch: bool,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AskArgs {
    pub role: String,
    pub task: String,
    pub message: Option<String>,
    pub file: Option<String>,
    pub diff: bool,
    pub test: bool,
    pub new: bool,
    pub force: bool,
    pub dry_run: bool,
    pub json: bool,
}

#[derive(Debug)]
pub struct Parsed {
    pub config: Option<String>,
    pub command: Command,
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Parsed> {
    let mut config = None;
    let mut words: Vec<String> = Vec::new();
    let mut flags: Vec<String> = Vec::new();
    let mut values: Vec<(String, String)> = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" | "--brief" | "--base" | "-m" | "--message" | "-f" | "--file" => {
                let value = args
                    .next()
                    .ok_or_else(|| TeamError::new(format!("{arg} necesita un valor")))?;
                if arg == "--config" {
                    config = Some(value);
                } else {
                    values.push((arg, value));
                }
            }
            "-h" | "--help" => {
                return Ok(Parsed {
                    config,
                    command: Command::Help,
                });
            }
            "-V" | "--version" => {
                return Ok(Parsed {
                    config,
                    command: Command::Version,
                });
            }
            flag if flag.starts_with('-') && flag.len() > 1 => flags.push(arg),
            _ => words.push(arg),
        }
    }
    let Some(name) = words.first().cloned() else {
        return Ok(Parsed {
            config,
            command: Command::Help,
        });
    };
    let positional = &words[1..];
    let mut used_flags: Vec<&str> = Vec::new();
    let mut flag = |name: &'static str| {
        used_flags.push(name);
        flags.iter().any(|f| f == name)
    };
    let value = |names: &[&str]| {
        values
            .iter()
            .find(|(k, _)| names.contains(&k.as_str()))
            .map(|(_, v)| v.clone())
    };
    let need = |count: usize, usage: &str| -> Result<()> {
        if positional.len() == count {
            Ok(())
        } else {
            Err(TeamError::new(format!("Uso: titan-team {usage}")))
        }
    };
    let command = match name.as_str() {
        "doc" => {
            need(0, "doc")?;
            Command::Doc
        }
        "check" => {
            need(0, "check")?;
            Command::Check
        }
        "init" => {
            need(0, "init [--force]")?;
            Command::Init {
                force: flag("--force"),
            }
        }
        "roles" => {
            need(0, "roles [--json]")?;
            Command::Roles {
                json: flag("--json"),
            }
        }
        "new" => {
            need(1, "new <encargo> --brief FICHERO [--base RAMA]")?;
            Command::New {
                task: positional[0].clone(),
                brief: value(&["--brief"])
                    .ok_or_else(|| TeamError::new("new necesita --brief FICHERO"))?,
                base: value(&["--base"]),
            }
        }
        "ask" => {
            need(2, "ask <rol> <encargo> [-m TEXTO | -f FICHERO] [opciones]")?;
            let message = value(&["-m", "--message"]);
            let file = value(&["-f", "--file"]);
            if message.is_some() && file.is_some() {
                return Err(TeamError::new("-m y -f no se pueden usar a la vez"));
            }
            Command::Ask(AskArgs {
                role: positional[0].clone(),
                task: positional[1].clone(),
                message,
                file,
                diff: flag("--diff"),
                test: flag("--test"),
                new: flag("--new"),
                force: flag("--force"),
                dry_run: flag("--dry-run"),
                json: flag("--json"),
            })
        }
        "show" => {
            need(1, "show <encargo> [--json]")?;
            Command::Show {
                task: positional[0].clone(),
                json: flag("--json"),
            }
        }
        "diff" => {
            need(1, "diff <encargo> [--stat]")?;
            Command::Diff {
                task: positional[0].clone(),
                stat: flag("--stat"),
            }
        }
        "test" => {
            need(1, "test <encargo>")?;
            Command::Test {
                task: positional[0].clone(),
            }
        }
        "note" => {
            need(1, "note <encargo> -m TEXTO")?;
            Command::Note {
                task: positional[0].clone(),
                text: value(&["-m", "--message"])
                    .filter(|t| !t.trim().is_empty())
                    .ok_or_else(|| TeamError::new("note necesita -m TEXTO"))?,
            }
        }
        "close" => {
            need(1, "close <encargo> [--delete-branch]")?;
            Command::Close {
                task: positional[0].clone(),
                delete_branch: flag("--delete-branch"),
            }
        }
        other => {
            return Err(TeamError::new(format!(
                "Orden desconocida: {other} (titan-team --help)"
            )));
        }
    };
    if let Some(unknown) = flags.iter().find(|f| !used_flags.contains(&f.as_str())) {
        return Err(TeamError::new(format!(
            "Opción desconocida para {name}: {unknown}"
        )));
    }
    let allowed_values: &[&str] = match name.as_str() {
        "new" => &["--brief", "--base"],
        "ask" => &["-m", "--message", "-f", "--file"],
        "note" => &["-m", "--message"],
        _ => &[],
    };
    if let Some((unknown, _)) = values
        .iter()
        .find(|(k, _)| !allowed_values.contains(&k.as_str()))
    {
        return Err(TeamError::new(format!(
            "Opción desconocida para {name}: {unknown}"
        )));
    }
    Ok(Parsed { config, command })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(line: &str) -> Result<Parsed> {
        parse(line.split_whitespace().map(str::to_string))
    }

    #[test]
    fn ask_takes_its_flags_anywhere() {
        let parsed =
            parse_words("--config x/team.toml ask revisor t1 --dry-run -m hola --json").unwrap();
        assert_eq!(parsed.config.as_deref(), Some("x/team.toml"));
        let Command::Ask(ask) = parsed.command else {
            panic!("not ask")
        };
        assert_eq!((ask.role.as_str(), ask.task.as_str()), ("revisor", "t1"));
        assert_eq!(ask.message.as_deref(), Some("hola"));
        assert!(ask.dry_run && ask.json && !ask.diff);
    }

    #[test]
    fn mistakes_are_explained() {
        for line in [
            "ask revisor",
            "ask r t -m a -f b",
            "new t1",
            "roles --stat",
            "show t1 --brief x",
            "frobnicate",
            "note t1",
        ] {
            let error = parse_words(line).unwrap_err();
            assert_eq!(error.code, 2, "{line}");
            assert!(!error.message.is_empty());
        }
        assert_eq!(parse_words("").unwrap().command, Command::Help);
    }
}
