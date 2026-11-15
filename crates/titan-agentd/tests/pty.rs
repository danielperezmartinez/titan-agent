#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use titan_agentd::pty::{PtyError, PtyHandler, PtyProcess, PtySpawn};

const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(10);

#[derive(Default)]
struct Observation {
    bytes: Vec<u8>,
    exits: usize,
    output_after_exit: bool,
    callback_thread: Option<ThreadId>,
    different_callback_threads: bool,
}

impl Observation {
    fn record_thread(&mut self) {
        let current = thread::current().id();
        if let Some(previous) = self.callback_thread {
            self.different_callback_threads |= previous != current;
        } else {
            self.callback_thread = Some(current);
        }
    }
}

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Observation>>);

impl PtyHandler for Recorder {
    fn output(&mut self, bytes: &[u8]) {
        let mut observation = self.0.lock().unwrap();
        observation.record_thread();
        observation.output_after_exit |= observation.exits != 0;
        observation.bytes.extend_from_slice(bytes);
    }

    fn exited(&mut self) {
        let mut observation = self.0.lock().unwrap();
        observation.record_thread();
        observation.exits += 1;
    }
}

impl Recorder {
    fn wait_for_output(&self, expected: &[u8]) {
        wait_until("expected PTY output", || {
            self.0
                .lock()
                .unwrap()
                .bytes
                .windows(expected.len())
                .any(|window| window == expected)
        });
    }

    fn wait_for_exit(&self) {
        wait_until("exited callback", || self.0.lock().unwrap().exits > 0);
    }

    fn assert_finished(&self) {
        let observation = self.0.lock().unwrap();
        assert_eq!(observation.exits, 1, "exited must be called exactly once");
        assert!(!observation.output_after_exit);
        assert!(!observation.different_callback_threads);
        assert_ne!(observation.callback_thread, Some(thread::current().id()));
    }
}

fn wait_until(description: &str, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if predicate() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        thread::sleep(POLL);
    }
}

fn command(argv: &[&str]) -> PtySpawn {
    PtySpawn {
        argv: argv.iter().map(|argument| (*argument).to_owned()).collect(),
        cwd: None,
        env: Vec::new(),
        cols: 80,
        rows: 24,
    }
}

fn shell(script: &str) -> PtySpawn {
    command(&["/bin/sh", "-c", script])
}

// Keep cleanup bounded even when a broken implementation hangs in kill or Drop.
struct Running {
    process: Arc<PtyProcess>,
    recorder: Recorder,
}

impl Running {
    fn spawn(spec: PtySpawn) -> Self {
        let recorder = Recorder::default();
        let process = PtyProcess::spawn(spec, recorder.clone()).unwrap();
        Self {
            process: Arc::new(process),
            recorder,
        }
    }

    fn kill(&self, grace: Duration) -> Duration {
        let process = Arc::clone(&self.process);
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let started = Instant::now();
            process.kill(grace);
            let _ = sender.send(started.elapsed());
        });
        receiver
            .recv_timeout(TIMEOUT)
            .expect("kill must finish within the deadline")
    }

    fn finish(&self) {
        self.recorder.wait_for_exit();
        wait_until("has_exited", || self.process.has_exited());
        self.kill(Duration::from_millis(100));
        assert!(self.process.has_exited());
        self.recorder.assert_finished();
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let process = Arc::clone(&self.process);
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            process.kill(Duration::from_millis(100));
            let _ = sender.send(());
        });
        let result = receiver.recv_timeout(TIMEOUT);
        if !thread::panicking() {
            assert!(result.is_ok(), "PTY cleanup exceeded its deadline");
        }
    }
}

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        loop {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("titan-pty-acceptance-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(fs::canonicalize(path).unwrap()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("could not create temporary directory: {error}"),
            }
        }
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

#[test]
fn public_types_support_thread_sharing_and_standard_errors() {
    fn assert_send_sync<T: Send + Sync>() {}
    fn assert_error<T: std::error::Error>() {}
    assert_send_sync::<PtyProcess>();
    assert_error::<PtyError>();
    assert_eq!(PtyProcess::INPUT_QUEUE, 256);
    let spec = command(&["printf", "hola"]);
    let cloned = spec.clone();
    assert_eq!(cloned.argv, spec.argv);
}

#[test]
fn path_lookup_prints_output_before_exactly_one_exit_callback() {
    let running = Running::spawn(command(&["printf", "hola"]));
    assert!(running.process.pid() > 0);
    running.recorder.wait_for_output(b"hola");
    running.finish();
}

#[test]
fn output_preserves_raw_bytes_and_order() {
    let running = Running::spawn(shell("printf 'first\\001'; printf '\\377last'"));
    running.recorder.wait_for_output(b"first\x01\xfflast");
    running.finish();
}

#[test]
fn a_program_without_output_still_reports_exit() {
    let running = Running::spawn(shell("exit 0"));
    running.finish();
    assert!(running.recorder.0.lock().unwrap().bytes.is_empty());
}

#[test]
fn queued_input_reaches_the_program_in_order() {
    let running = Running::spawn(shell("printf 'ready\\n'; read l; echo \"got:$l\""));
    running.recorder.wait_for_output(b"ready");
    assert!(!running.process.has_exited());
    running.process.write(b"a").unwrap();
    running.process.write(b"b").unwrap();
    running.process.write(b"c\r").unwrap();
    running.recorder.wait_for_output(b"got:abc");
    running.finish();
}

#[test]
fn initial_terminal_size_uses_rows_then_columns() {
    let mut spec = shell("stty size");
    spec.cols = 30;
    spec.rows = 10;
    let running = Running::spawn(spec);
    running.recorder.wait_for_output(b"10 30");
    running.finish();
}

#[test]
fn resizing_changes_the_size_seen_by_the_program() {
    let mut spec = shell("printf 'ready\\n'; read x; stty size");
    spec.cols = 30;
    spec.rows = 10;
    let running = Running::spawn(spec);
    running.recorder.wait_for_output(b"ready");
    running.process.resize(100, 40).unwrap();
    running.process.write(b"go\r").unwrap();
    running.recorder.wait_for_output(b"40 100");
    running.finish();
}

#[test]
fn resizing_notifies_the_foreground_program_with_sigwinch() {
    let running = Running::spawn(shell(
        "trap 'stty size; exit 0' WINCH; printf 'ready\\n'; while :; do read x; done",
    ));
    running.recorder.wait_for_output(b"ready");
    running.process.resize(101, 41).unwrap();
    running.recorder.wait_for_output(b"41 101");
    running.finish();
}

#[test]
fn zero_initial_dimensions_are_clamped_to_one() {
    let mut spec = shell("stty size");
    spec.cols = 0;
    spec.rows = 0;
    let running = Running::spawn(spec);
    running.recorder.wait_for_output(b"1 1");
    running.finish();
}

#[test]
fn zero_resize_dimensions_are_clamped_to_one() {
    let running = Running::spawn(shell("printf 'ready\\n'; read x; stty size"));
    running.recorder.wait_for_output(b"ready");
    running.process.resize(0, 0).unwrap();
    running.process.write(b"go\r").unwrap();
    running.recorder.wait_for_output(b"1 1");
    running.finish();
}

#[test]
fn default_terminal_environment_is_set() {
    let running = Running::spawn(shell("printf '%s|%s' \"$TERM\" \"$COLORTERM\""));
    running
        .recorder
        .wait_for_output(b"xterm-256color|truecolor");
    running.finish();
}

#[test]
fn environment_additions_can_override_both_terminal_defaults() {
    let mut spec = shell("printf '%s|%s|%s' \"$TERM\" \"$COLORTERM\" \"$PTY_SAMPLE\"");
    spec.env = vec![
        ("TERM".into(), "sample-terminal".into()),
        ("COLORTERM".into(), "sample-color".into()),
        ("PTY_SAMPLE".into(), "sample-value".into()),
    ];
    let running = Running::spawn(spec);
    running
        .recorder
        .wait_for_output(b"sample-terminal|sample-color|sample-value");
    running.finish();
}

#[test]
fn the_program_inherits_the_daemons_environment() {
    let expected = std::env::var("PATH").expect("POSIX utilities require PATH for these tests");
    let running = Running::spawn(shell("printf '<%s>' \"$PATH\""));
    running
        .recorder
        .wait_for_output(format!("<{expected}>").as_bytes());
    running.finish();
}

#[test]
fn explicit_working_directory_is_used() {
    let directory = TemporaryDirectory::new();
    let mut spec = command(&["pwd"]);
    spec.cwd = Some(directory.0.clone());
    let running = Running::spawn(spec);
    running
        .recorder
        .wait_for_output(directory.0.to_str().unwrap().as_bytes());
    running.finish();
}

#[test]
fn omitted_working_directory_uses_the_daemons_directory() {
    let expected = fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
    let running = Running::spawn(command(&["pwd"]));
    running
        .recorder
        .wait_for_output(expected.to_str().unwrap().as_bytes());
    running.finish();
}

#[test]
fn empty_argv_is_rejected() {
    let result = PtyProcess::spawn(command(&[]), Recorder::default());
    assert!(matches!(result, Err(PtyError::EmptyCommand)));
}

#[test]
fn nonexistent_program_is_a_spawn_error_or_exits_promptly() {
    let directory = TemporaryDirectory::new();
    let missing = directory.0.join("missing-program");
    let recorder = Recorder::default();
    match PtyProcess::spawn(command(&[missing.to_str().unwrap()]), recorder.clone()) {
        Err(PtyError::Spawn(_)) => {}
        Ok(process) => {
            let running = Running {
                process: Arc::new(process),
                recorder,
            };
            running.finish();
            assert!(matches!(
                running.process.write(b"late"),
                Err(PtyError::Closed)
            ));
        }
        Err(error) => panic!("unexpected error for missing program: {error}"),
    }
}

#[test]
fn natural_exit_closes_input_and_kill_is_idempotent() {
    let running = Running::spawn(shell("printf 'done'"));
    running.finish();
    assert!(matches!(
        running.process.write(b"late"),
        Err(PtyError::Closed)
    ));
    running.kill(Duration::from_millis(200));
    running.kill(Duration::ZERO);
    running.recorder.assert_finished();
}

#[test]
fn kill_allows_a_program_to_handle_sighup_before_returning() {
    let running = Running::spawn(shell(
        "trap 'printf hup-handled; exit 0' HUP; printf 'ready\\n'; while :; do read x; done",
    ));
    running.recorder.wait_for_output(b"ready");
    running.kill(Duration::from_secs(2));
    running.recorder.wait_for_output(b"hup-handled");
    assert!(running.process.has_exited());
    running.recorder.assert_finished();
    assert!(matches!(
        running.process.write(b"late"),
        Err(PtyError::Closed)
    ));
}

#[test]
fn kill_escalates_after_grace_when_sighup_is_ignored() {
    let running = Running::spawn(shell("trap '' HUP; printf 'ready\\n'; sleep 100"));
    running.recorder.wait_for_output(b"ready");
    let grace = Duration::from_millis(200);
    let elapsed = running.kill(grace);
    assert!(elapsed >= grace, "SIGKILL must wait for the grace period");
    assert!(elapsed < Duration::from_secs(3), "kill took {elapsed:?}");
    assert!(running.process.has_exited());
    running.recorder.assert_finished();
    assert!(matches!(
        running.process.write(b"late"),
        Err(PtyError::Closed)
    ));
    running.kill(grace);
    running.recorder.assert_finished();
}

#[test]
fn input_queue_reports_full_without_blocking_the_caller() {
    let running = Running::spawn(shell(
        "stty -echo -icanon; printf 'ready\\n'; exec sleep 100",
    ));
    running.recorder.wait_for_output(b"ready");
    let process = Arc::clone(&running.process);
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let chunk = vec![b'x'; 64 * 1024];
        let deadline = Instant::now() + Duration::from_secs(2);
        let result = loop {
            match process.write(&chunk) {
                Ok(()) if Instant::now() < deadline => {}
                Ok(()) => break Ok(()),
                Err(error) => break Err(error),
            }
        };
        let _ = sender.send(result);
    });
    let result = receiver
        .recv_timeout(TIMEOUT)
        .expect("write must never block when the program does not read");
    assert!(matches!(result, Err(PtyError::QueueFull)));
    running.kill(Duration::from_millis(100));
    assert!(running.process.has_exited());
    running.recorder.assert_finished();
}

#[test]
fn dropping_a_live_process_finishes_the_reader() {
    let recorder = Recorder::default();
    let process = PtyProcess::spawn(
        shell("trap '' HUP; printf 'ready\\n'; exec sleep 100"),
        recorder.clone(),
    )
    .unwrap();
    recorder.wait_for_output(b"ready");
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        drop(process);
        let _ = sender.send(());
    });
    receiver
        .recv_timeout(TIMEOUT)
        .expect("Drop must terminate the child and join its threads");
    recorder.assert_finished();
}
