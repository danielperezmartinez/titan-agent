use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use titan_agent_core::paths::{SOCKET_ENV, lock_path, socket_path};

#[test]
fn socket_environment_variable_name_is_stable() {
    assert_eq!(SOCKET_ENV, "TITAN_AGENT_SOCKET");
}

#[test]
fn nonempty_override_takes_precedence() {
    assert_eq!(
        socket_path(
            Some(OsStr::new("/run/sample/custom.sock")),
            Some(OsStr::new("/run/sample/runtime")),
            42
        ),
        PathBuf::from("/run/sample/custom.sock")
    );
    assert_eq!(
        socket_path(Some(OsStr::new("custom.sock")), None, 42),
        PathBuf::from("custom.sock")
    );
}

#[test]
fn runtime_directory_is_used_for_absent_or_empty_override() {
    for override_path in [None, Some(OsStr::new(""))] {
        for runtime in ["/run/sample", "/run/sample/"] {
            assert_eq!(
                socket_path(override_path, Some(OsStr::new(runtime)), 42),
                PathBuf::from("/run/sample/titan-agent/agentd.sock")
            );
        }
    }
}

#[test]
fn fallback_includes_uid_when_both_candidates_are_absent_or_empty() {
    for override_path in [None, Some(OsStr::new(""))] {
        for runtime in [None, Some(OsStr::new(""))] {
            for uid in [0, 42, u32::MAX] {
                assert_eq!(
                    socket_path(override_path, runtime, uid),
                    PathBuf::from(format!("/tmp/titan-agent-{uid}/agentd.sock"))
                );
            }
        }
    }
}

#[test]
fn lock_suffix_is_appended_to_the_entire_socket_path() {
    for (socket, expected) in [
        ("/run/sample/agentd.sock", "/run/sample/agentd.sock.lock"),
        ("custom", "custom.lock"),
        ("custom.lock", "custom.lock.lock"),
    ] {
        assert_eq!(lock_path(Path::new(socket)), PathBuf::from(expected));
    }
}

#[cfg(unix)]
#[test]
fn paths_preserve_non_utf8_os_strings() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let override_path = OsStr::from_bytes(b"/run/sample/\xff.sock");
    assert_eq!(
        socket_path(Some(override_path), None, 42),
        PathBuf::from(override_path)
    );
    let runtime = OsStr::from_bytes(b"/run/\xff");
    assert_eq!(
        socket_path(None, Some(runtime), 42),
        PathBuf::from(std::ffi::OsString::from_vec(
            b"/run/\xff/titan-agent/agentd.sock".to_vec()
        ))
    );
    assert_eq!(
        lock_path(Path::new(override_path)),
        PathBuf::from(std::ffi::OsString::from_vec(
            b"/run/sample/\xff.sock.lock".to_vec()
        ))
    );
}
