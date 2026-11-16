//! Running other programs: `git` and the test command (with a time limit),
//! and the path patterns of `allowed_paths` / `forbidden_paths`.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::Failure;

/// Runs `git -C <repo> <args>`; an error status becomes a [`Failure`] with
/// git's own message.
pub fn git(repo: &Path, args: &[&str]) -> Result<String, Failure> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| Failure::usage("git no está disponible en PATH"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        return Err(Failure::usage(if detail.is_empty() {
            format!(
                "git terminó con exit {}",
                output.status.code().unwrap_or(-1)
            )
        } else {
            detail
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs git and only reports whether it succeeded.
pub fn git_ok(repo: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Runs `command` with the system shell in `dir`, killing it after
/// `limit`. Returns the exit code (124 on timeout) and its whole combined
/// output. Whatever the command left running in the background is ended
/// with it, so its open pipes cannot outlive the limit.
pub fn run_shell(command: &str, dir: &Path, limit: Duration) -> (i32, String) {
    let mut shell = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C");
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c");
        c
    };
    own_group(&mut shell);
    let child = shell
        .arg(command)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => return (127, format!("no se pudo ejecutar el test: {error}")),
    };
    let readers = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .map(|stream| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut stream) = stream {
                let _ = stream.read_to_end(&mut bytes);
            }
            bytes
        })
    });
    let deadline = Instant::now() + limit;
    let mut code = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                kill_tree(&mut child);
                break status.code().unwrap_or(1);
            }
            Ok(None) if Instant::now() >= deadline => {
                kill_tree(&mut child);
                break 124;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break 1,
        }
    };
    let mut text = String::new();
    for reader in readers {
        text.push_str(&String::from_utf8_lossy(&reader.join().unwrap_or_default()));
    }
    if code == 124 {
        text.push_str(&format!("\nTests agotaron {} s", limit.as_secs()));
    } else if code < 0 {
        code = 1;
    }
    (code, text)
}

/// The last `count` lines of `text`, for a prompt.
pub fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

/// Starts the program in its own process group, so a timeout can end it
/// together with everything it started.
pub fn own_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = command;
}

/// Kills a child started with [`own_group`] and its process group.
pub fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// `fnmatch`-style match over chars: `*` is any run of chars (slashes
/// included), `?` one char, `[abc]`, `[a-z]` and `[!abc]` a class. A
/// pattern ending in `/` covers everything below that folder.
pub fn path_matches(file: &str, pattern: &str) -> bool {
    let pattern = if pattern.ends_with('/') {
        format!("{pattern}*")
    } else {
        pattern.to_string()
    };
    let text: Vec<char> = file.chars().collect();
    let pattern: Vec<char> = pattern.chars().collect();
    glob(&text, &pattern)
}

/// Matches one class starting at `pattern[0] == '['`: `Some((matched,
/// length))`, or `None` when the `[` is not closed (then it is literal).
fn class(c: char, pattern: &[char]) -> Option<(bool, usize)> {
    let mut i = 1;
    let negated = matches!(pattern.get(i), Some('!') | Some('^'));
    if negated {
        i += 1;
    }
    let mut matched = false;
    let mut first = true;
    while i < pattern.len() {
        if pattern[i] == ']' && !first {
            return Some((matched != negated, i + 1));
        }
        first = false;
        if i + 2 < pattern.len() && pattern[i + 1] == '-' && pattern[i + 2] != ']' {
            matched |= pattern[i] <= c && c <= pattern[i + 2];
            i += 3;
        } else {
            matched |= pattern[i] == c;
            i += 1;
        }
    }
    None
}

fn glob(text: &[char], pattern: &[char]) -> bool {
    let (mut t, mut p) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        let step = match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
                continue;
            }
            Some('?') => Some(1),
            Some('[') => match class(text[t], &pattern[p..]) {
                Some((true, len)) => Some(len),
                Some((false, _)) => None,
                None => (text[t] == '[').then_some(1),
            },
            Some(c) => (*c == text[t]).then_some(1),
            None => None,
        };
        match step {
            Some(len) => {
                t += 1;
                p += len;
            }
            None => match star {
                Some((sp, st)) => {
                    p = sp + 1;
                    t = st + 1;
                    star = Some((sp, st + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// Warnings for the files a turn changed.
pub fn path_warnings(allowed: &[String], forbidden: &[String], changed: &[String]) -> Vec<String> {
    let mut warnings = Vec::new();
    for file in changed {
        if !allowed.is_empty() && !allowed.iter().any(|p| path_matches(file, p)) {
            warnings.push(format!("Fichero fuera de allowed_paths: {file}"));
        }
        if forbidden.iter().any(|p| path_matches(file, p)) {
            warnings.push(format!("Fichero dentro de forbidden_paths: {file}"));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_behave_like_fnmatch() {
        assert!(path_matches("crates/a/tests/x.rs", "crates/*/tests/"));
        assert!(path_matches("crates/a/b/tests/x.rs", "crates/*/tests/"));
        assert!(!path_matches("crates/a/src/x.rs", "crates/*/tests/"));
        assert!(path_matches("docs/a.md", "docs/"));
        assert!(path_matches("a.rs", "?.rs"));
        assert!(!path_matches("ab.rs", "?.rs"));
        assert!(path_matches("ñ.rs", "?.rs"));
        assert!(path_matches("v2.rs", "v[0-9].rs"));
        assert!(!path_matches("vx.rs", "v[0-9].rs"));
        assert!(path_matches("b.md", "[!a].md"));
        assert!(!path_matches("a.md", "[!a].md"));
        assert!(path_matches("[x", "[x"));
    }

    #[test]
    fn warnings_name_each_file_and_list() {
        let warnings = path_warnings(
            &["crates/*/tests/".into()],
            &["docs/".into()],
            &["crates/x/tests/a.rs".into(), "docs/n.md".into()],
        );
        assert_eq!(
            warnings,
            [
                "Fichero fuera de allowed_paths: docs/n.md",
                "Fichero dentro de forbidden_paths: docs/n.md"
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_shell_command_is_killed_at_its_limit() {
        let started = Instant::now();
        let (code, _) = run_shell("sleep 5 & echo bg", Path::new("."), Duration::from_secs(5));
        assert_eq!(code, 0);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "a background child must not hold it"
        );
        let (code, out) = run_shell(
            "echo hi; sleep 5; echo late",
            Path::new("."),
            Duration::from_millis(300),
        );
        assert_eq!(code, 124);
        assert!(out.contains("Tests agotaron"));
        assert_eq!(
            run_shell("exit 3", Path::new("."), Duration::from_secs(5)).0,
            3
        );
    }
}
