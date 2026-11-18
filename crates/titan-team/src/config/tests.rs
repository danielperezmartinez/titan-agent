use super::*;

fn errors(text: &str) -> Vec<(String, &'static str)> {
    parse(text, Path::new("/nonexistent"))
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.path, e.code))
        .collect()
}

#[test]
fn checks_need_a_writing_role_and_their_commands() {
    let found = errors(
        "version = 1\nname = \"x\"\n[project]\ntest_cmd = \"t\"\n\
         [roles.w]\ncli = \"codex\"\nmodel = \"m\"\nlaunch = false\nsandbox = \"workspace-write\"\n\
         checks = [\"fmt\", \"lint\", \"test\"]\nchecks_required = \"yes\"\n\
         [roles.r]\ncli = \"codex\"\nmodel = \"m\"\nlaunch = false\nchecks = [\"gate\"]\n",
    );
    assert_eq!(
        found,
        [
            ("roles.w.checks".to_string(), "unknown_value"),
            ("roles.w.checks_required".to_string(), "bad_type"),
            ("roles.r.checks".to_string(), "unknown_value"),
            ("project.fmt_cmd".to_string(), "missing_key"),
            ("project.gate_cmd".to_string(), "missing_key"),
        ]
    );
    let team = parse(
        "version = 1\nname = \"x\"\n[project]\nfmt_cmd = \"f\"\ngate_cmd = \"g\"\n\
         [roles.w]\ncli = \"codex\"\nmodel = \"m\"\nlaunch = false\nsandbox = \"workspace-write\"\n\
         checks = [\"gate\", \"fmt\"]\nchecks_required = true\n",
        Path::new("/nonexistent"),
    )
    .unwrap();
    let role = team.role("w").unwrap();
    assert_eq!(role.checks, ["gate", "fmt"]);
    assert_eq!(role.checks_required, ["gate", "fmt"]);
    assert_eq!(team.project.check_cmd("gate"), Some("g"));
    assert_eq!(team.project.check_cmd("test"), None);
}

#[test]
fn checks_required_takes_a_list_of_its_checks() {
    let toml = |required: &str| {
        format!(
            "version = 1\nname = \"x\"\n[project]\nbuild_cmd = \"b\"\ntest_cmd = \"t\"\n\
             [roles.w]\ncli = \"codex\"\nmodel = \"m\"\nlaunch = false\nsandbox = \"workspace-write\"\n\
             checks = [\"build\", \"test\"]\n{required}\n"
        )
    };
    let team = parse(
        &toml("checks_required = [\"build\"]\nbrief_sections = [\"Contrato\"]"),
        Path::new("/nonexistent"),
    )
    .unwrap();
    let role = team.role("w").unwrap();
    assert_eq!(role.checks_required, ["build"]);
    assert_eq!(role.brief_sections, ["Contrato"]);
    assert_eq!(team.project.check_cmd("build"), Some("b"));
    let role = parse(&toml(""), Path::new("/nonexistent")).unwrap();
    assert!(role.role("w").unwrap().checks_required.is_empty());
    assert_eq!(
        errors(&toml(
            "checks_required = [\"gate\"]\nbrief_sections = [\"Contrato\", \" \"]"
        )),
        [
            ("roles.w.checks_required".to_string(), "unknown_value"),
            ("roles.w.brief_sections".to_string(), "bad_type"),
        ]
    );
    assert_eq!(
        errors(&toml("checks_required = [1]")),
        [("roles.w.checks_required".to_string(), "bad_type")]
    );
}

#[test]
fn roles_keep_file_order_and_the_first_claude_role_directs() {
    let text = "version = 1\nname = \"t\"\n[roles.zeta]\ncli = \"codex\"\nmodel = \"m\"\nlaunch = false\n\
                [roles.alpha]\ncli = \"claude\"\nmodel = \"opus\"\nlaunch = false\n";
    let team = parse(text, Path::new("/x")).unwrap();
    let names: Vec<&str> = team.roles.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["zeta", "alpha"]);
    assert_eq!(team.director(), Some("alpha"));
    assert_eq!(team.state_dir, PathBuf::from("/x/.agents/state"));
    assert_eq!(team.role("zeta").unwrap().sandbox, "read-only");
}

#[test]
fn types_values_and_references_are_reported_with_their_paths() {
    let found = errors(
        "version = 1\nname = 3\n[flow]\nsteps = [\"ghost\"]\nmax_review_rounds = -1\n\
         [roles.a]\ncli = \"gemini\"\nmodel = \"m\"\neffort = \"huge\"\ntimeout_s = \"9\"\n[roles.B]\ncli = \"codex\"\nmodel = \"\"\nlaunch = false\n",
    );
    for expected in [
        ("name", "bad_type"),
        ("flow.steps", "unknown_role"),
        ("flow.max_review_rounds", "bad_type"),
        ("roles.a.cli", "unknown_value"),
        ("roles.a.effort", "unknown_value"),
        ("roles.a.timeout_s", "bad_type"),
        ("roles.B", "bad_type"),
        ("roles.B.model", "bad_type"),
    ] {
        assert!(
            found.contains(&(expected.0.to_string(), expected.1)),
            "{expected:?} in {found:?}"
        );
    }
    assert_eq!(
        errors("name = \"x\"\n[roles]\n")[0],
        ("version".into(), "missing_key")
    );
    let found = errors(
        "version = 1\nname = \"x\"\n[codex]\nconfig = [\"a=1\", \"nope\"]\n[roles.r]\ncli = \"codex\"\nmodel = \"m\"\nlaunch = false\nconfig = [\"=x\"]\n",
    );
    assert!(
        found.contains(&("codex.config".into(), "bad_type")),
        "{found:?}"
    );
    assert!(
        found.contains(&("roles.r.config".into(), "bad_type")),
        "{found:?}"
    );
}

#[test]
fn resolve_drops_dot_components() {
    assert_eq!(
        resolve(Path::new("/a/b"), Path::new("./c/../d")),
        PathBuf::from("/a/b/d")
    );
    assert_eq!(
        resolve(Path::new("/a"), Path::new("/abs/./x")),
        PathBuf::from("/abs/x")
    );
}
