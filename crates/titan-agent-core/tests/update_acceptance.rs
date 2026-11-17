use serde_json::json;
use titan_agent_core::protocol::TerminalRef;
use titan_agent_core::session::Session;
use titan_agent_core::update::{
    Artifact, BINARIES, EntryKind, MAX_ARTIFACT_BYTES, Manifest, RELEASES_URL, Selected,
    UpdateError, Version, apply_blockers, bundle_root, check_bundle, check_download, check_entry,
    manifest_url, safe_to_apply, select, sha256_hex, signature_url, trusted_comment,
    verify_manifest,
};

fn manifest() -> Manifest {
    Manifest {
        format: 1,
        version: "0.6.1".into(),
        published: "2026-11-20T10:00:00Z".into(),
        notes_url: "https://example.invalid/notes".into(),
        artifacts: vec![Artifact {
            target: "linux-x86_64".into(),
            name: "titan-agent-0.6.1-linux-x86_64.tar.gz".into(),
            sha256: sha256_hex(b"abc"),
            size: 3,
        }],
    }
}
fn version(s: &str) -> Version {
    Version::parse(s).unwrap()
}
fn error<T: std::fmt::Debug>(result: Result<T, UpdateError>, code: &str) {
    assert_eq!(result.unwrap_err().code(), code);
}

#[test]
fn c1_manifest_requires_exact_signed_bytes_schema_format_and_trusted_comment() {
    let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let other = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let key = kp.pk.to_base64();
    let other_key = other.pk.to_base64();
    let sign = |bytes: &[u8], comment: &str| -> String {
        minisign::sign(
            Some(&kp.pk),
            &kp.sk,
            std::io::Cursor::new(bytes),
            Some(comment),
            None,
        )
        .unwrap()
        .into_string()
    };
    assert_eq!(
        trusted_comment("0.6.1"),
        "titan-agent-update format=1 version=0.6.1"
    );
    let mut value = serde_json::to_value(manifest()).unwrap();
    value["future_field"] = json!(true);
    let bytes = serde_json::to_vec(&value).unwrap();
    let sig = sign(&bytes, &trusted_comment("0.6.1"));
    assert_eq!(
        verify_manifest(&bytes, &sig, &[&other_key, &key]).unwrap(),
        manifest()
    );
    error(verify_manifest(&bytes, &sig, &[&other_key]), "signature");
    let mut changed = bytes.clone();
    changed.push(b' ');
    error(verify_manifest(&changed, &sig, &[&key]), "signature");
    error(
        verify_manifest(&bytes, "unreadable signature", &[&key]),
        "signature",
    );
    error(
        verify_manifest(&bytes, &sign(&bytes, &trusted_comment("0.6.2")), &[&key]),
        "signature",
    );
    for invalid in [b"not json".as_slice(), b"{}".as_slice()] {
        error(
            verify_manifest(invalid, &sign(invalid, &trusted_comment("0.6.1")), &[&key]),
            "manifest",
        );
    }
    value["format"] = json!(2);
    let bytes = serde_json::to_vec(&value).unwrap();
    error(
        verify_manifest(&bytes, &sign(&bytes, &trusted_comment("0.6.1")), &[&key]),
        "format",
    );
}

#[test]
fn c2_selection_enforces_order_limits_and_constructs_release_urls() {
    let base = "https://example.invalid/releases";
    assert_eq!(
        RELEASES_URL,
        "https://github.com/danielperezmartinez/titan-agent/releases"
    );
    assert_eq!(MAX_ARTIFACT_BYTES, 268_435_456);
    assert_eq!(
        manifest_url(base),
        format!("{base}/latest/download/titan-agent-update.json")
    );
    assert_eq!(
        signature_url(base),
        format!("{base}/latest/download/titan-agent-update.json.minisig")
    );
    assert_eq!(version("0.6.1").to_string(), "0.6.1");
    assert!(version("0.10.0") > version("0.9.9"));
    let choose =
        |m: &Manifest, installed, bad: &[Version]| select(m, installed, bad, "linux-x86_64", base);
    for text in [
        "v0.6.1",
        "0.6",
        "0.6.1-rc.1",
        "0.6.1+build",
        " 0.6.1",
        "0.6.1 ",
        "00.6.1",
        "0.06.1",
        "0.6.01",
        "",
        "0.-6.1",
    ] {
        assert!(Version::parse(text).is_none(), "{text:?}");
        let mut m = manifest();
        m.version = text.into();
        m.artifacts.clear();
        error(choose(&m, version("0.6.0"), &[]), "manifest");
    }
    for installed in ["0.6.1", "0.7.0"] {
        let mut m = manifest();
        m.artifacts.clear();
        error(
            choose(&m, version(installed), &[version("0.6.1")]),
            "not_newer",
        );
    }
    let mut m = manifest();
    m.artifacts.clear();
    error(
        choose(&m, version("0.6.0"), &[version("0.6.1")]),
        "bad_version",
    );
    error(choose(&m, version("0.6.0"), &[]), "no_artifact");
    for name in ["", "nested/archive", "nested\\archive", "archive..gz"] {
        let mut m = manifest();
        m.artifacts[0].name = name.into();
        m.artifacts[0].size = MAX_ARTIFACT_BYTES + 1;
        error(choose(&m, version("0.6.0"), &[]), "bad_name");
    }
    let mut m = manifest();
    m.artifacts[0].size = MAX_ARTIFACT_BYTES + 1;
    error(choose(&m, version("0.6.0"), &[]), "too_big");
    m.artifacts[0].size = MAX_ARTIFACT_BYTES;
    let selected = choose(&m, version("0.6.0"), &[]).unwrap();
    assert_eq!(
        selected,
        Selected {
            version: version("0.6.1"),
            artifact: m.artifacts[0].clone(),
            notes_url: m.notes_url.clone(),
            url: format!("{base}/download/v0.6.1/{}", m.artifacts[0].name)
        }
    );
}

#[test]
fn c3_download_checks_size_before_hash_and_accepts_hex_case() {
    let artifact = manifest().artifacts.remove(0);
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    check_download(3, &artifact.sha256.to_uppercase(), &artifact).unwrap();
    error(
        check_download(4, &"0".repeat(64), &artifact),
        "size_mismatch",
    );
    error(
        check_download(3, &"0".repeat(64), &artifact),
        "hash_mismatch",
    );
}

#[test]
fn c4_archive_entries_stay_inside_root_and_require_all_four_binaries() {
    let root = bundle_root(version("0.6.1"), "linux-x86_64");
    assert_eq!(root, "titan-agent-0.6.1-linux-x86_64");
    for (path, kind, link) in [
        (root.clone(), EntryKind::Dir, None),
        (format!("{root}/"), EntryKind::Dir, None),
        (format!("{root}/bin/tool"), EntryKind::File, None),
        (
            format!("{root}/bin/link"),
            EntryKind::Symlink,
            Some("../share/icon"),
        ),
    ] {
        check_entry(&root, &path, kind, link).unwrap();
    }
    check_entry(
        &root,
        &format!("{root}/bin/link"),
        EntryKind::Hardlink,
        Some(&format!("{root}/bin/tool")),
    )
    .unwrap();
    for path in [
        "/outside/file".into(),
        "../file".into(),
        "another-root/file".into(),
        format!("{root}/../file"),
        format!("{root}/bin\\file"),
    ] {
        error(
            check_entry(&root, &path, EntryKind::File, None),
            "unsafe_entry",
        );
    }
    for link in ["../../outside/file", "/outside/file"] {
        error(
            check_entry(
                &root,
                &format!("{root}/bin/link"),
                EntryKind::Symlink,
                Some(link),
            ),
            "unsafe_entry",
        );
    }
    error(
        check_entry(
            &root,
            &format!("{root}/bin/link"),
            EntryKind::Hardlink,
            Some("another-root/file"),
        ),
        "unsafe_entry",
    );
    error(
        check_entry(&root, &format!("{root}/fifo"), EntryKind::Other, None),
        "unsafe_entry",
    );
    assert_eq!(
        BINARIES,
        [
            "titan-agent",
            "titan-agentd",
            "titan-team",
            "titan-agent-launcher"
        ]
    );
    let files: Vec<String> = BINARIES.iter().map(|name| format!("bin/{name}")).collect();
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    check_bundle(&refs).unwrap();
    for missing in 0..4 {
        let remaining: Vec<&str> = refs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != missing)
            .map(|(_, p)| *p)
            .collect();
        error(check_bundle(&remaining), "incomplete");
    }
}

#[test]
fn c5_only_working_sessions_with_daemon_terminals_block_and_windows_always_block() {
    let make = |key: &str, agent: &str, state| -> Session {
        serde_json::from_value(json!({"key":key,"agent":agent,"cli_id":key,"name":"sample",
            "cwd":"/fictional/work","project_root":"/fictional/work","project":"work",
            "kind":"background","state":state}))
        .unwrap()
    };
    let sessions = vec![
        make("cx:z", "codex", json!("working")),
        make("cl:a", "claude_code", json!("working")),
        make("cl:detached", "claude_code", json!("working")),
        make("cl:wait", "claude_code", json!("waiting")),
        make("cx:need", "codex", json!("needs_you")),
        make("cx:done", "codex", json!({"finished":{"failed":false}})),
    ];
    let terminals: Vec<TerminalRef> = ["cx:z", "cl:a", "cl:a", "cl:wait", "cx:need", "cx:done"]
        .iter()
        .enumerate()
        .map(|(i, key)| TerminalRef {
            session: (*key).into(),
            terminal: format!("terminal-{i}"),
        })
        .collect();
    let blockers = apply_blockers(&sessions, &terminals);
    assert_eq!(blockers, vec!["cl:a", "cx:z"]);
    assert!(!safe_to_apply(0, &blockers));
    assert!(!safe_to_apply(1, &[]));
    assert!(apply_blockers(&sessions, &[]).is_empty());
    assert!(safe_to_apply(0, &[]));
}
