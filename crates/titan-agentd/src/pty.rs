//! Terminal processes on a pseudo-terminal.

use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::tty::{self, Options, Shell};
use rustix::event::{PollFd, PollFlags, poll};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use rustix::pipe;
use rustix::process::{Pid, Signal, WaitOptions, kill_process, kill_process_group, waitpid};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// What to run and how.
#[derive(Debug, Clone)]
pub struct PtySpawn {
    /// Program and arguments; argv[0] is looked up in PATH. Must not be empty.
    pub argv: Vec<String>,
    /// Working directory; None = the daemon's.
    pub cwd: Option<PathBuf>,
    /// Extra environment variables.
    pub env: Vec<(String, String)>,
    /// Initial size.
    pub cols: u16,
    /// Initial size.
    pub rows: u16,
}

/// Errors returned by PTY operations.
#[derive(Debug)]
pub enum PtyError {
    /// argv was empty.
    EmptyCommand,
    /// PTY creation or process startup failed.
    Spawn(std::io::Error),
    /// The input queue is full.
    QueueFull,
    /// The process has exited or was killed.
    Closed,
    /// An I/O operation failed.
    Io(std::io::Error),
}

impl std::fmt::Display for PtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyCommand => write!(f, "command is empty"),
            Self::Spawn(error) => write!(f, "failed to spawn PTY process: {error}"),
            Self::QueueFull => write!(f, "PTY input queue is full"),
            Self::Closed => write!(f, "PTY process is closed"),
            Self::Io(error) => write!(f, "PTY I/O failed: {error}"),
        }
    }
}
impl std::error::Error for PtyError {}

/// Callbacks, run on the reader thread.
pub trait PtyHandler: Send + 'static {
    /// Raw output bytes, in order, as they are read.
    fn output(&mut self, bytes: &[u8]);
    /// Called exactly once after the last output.
    fn exited(&mut self);
}

type Shared = Arc<SharedState>;
struct SharedState {
    exited: AtomicBool,
    killed: AtomicBool,
}

/// A process running in a pseudo-terminal.
pub struct PtyProcess {
    pid: u32,
    state: Shared,
    pty: Mutex<Option<tty::Pty>>,
    sender: Mutex<Option<mpsc::SyncSender<Vec<u8>>>>,
    reader: Mutex<Option<JoinHandle<()>>>,
    writer: Mutex<Option<JoinHandle<()>>>,
    wake: OwnedFd,
    kill_lock: Mutex<()>,
}

impl PtyProcess {
    /// Capacity of the input queue, in messages.
    pub const INPUT_QUEUE: usize = 256;

    /// Starts a program in a new PTY.
    pub fn spawn(spec: PtySpawn, mut handler: impl PtyHandler) -> Result<PtyProcess, PtyError> {
        if spec.argv.is_empty() {
            return Err(PtyError::EmptyCommand);
        }
        let mut env: HashMap<_, _> = spec.env.into_iter().collect();
        env.entry("TERM".to_owned())
            .or_insert_with(|| "xterm-256color".to_owned());
        env.entry("COLORTERM".to_owned())
            .or_insert_with(|| "truecolor".to_owned());
        let options = Options {
            shell: Some(Shell::new(spec.argv[0].clone(), spec.argv[1..].to_vec())),
            working_directory: spec.cwd,
            env,
            ..Options::default()
        };
        let size = WindowSize {
            num_lines: spec.rows.max(1),
            num_cols: spec.cols.max(1),
            cell_width: 1,
            cell_height: 1,
        };
        let pty = tty::new(&options, size, 0).map_err(PtyError::Spawn)?;
        let pid = pty.child().id();
        let process = process_pid(pid)
            .ok_or_else(|| PtyError::Spawn(std::io::Error::other("invalid child pid")))?;
        macro_rules! setup {
            ($result:expr) => {
                match $result {
                    Ok(value) => value,
                    Err(error) => {
                        shutdown_child(process, Duration::from_secs(2));
                        return Err(PtyError::Spawn(error.into()));
                    }
                }
            };
        }
        let (wake_read, wake_write) = setup!(pipe::pipe());
        let master = setup!(pty.file().try_clone());
        setup!(set_nonblocking(&master));
        let writer_file = setup!(pty.file().try_clone());
        setup!(set_nonblocking(&writer_file));
        let (sender, receiver) = mpsc::sync_channel(Self::INPUT_QUEUE);
        let state = Arc::new(SharedState {
            exited: AtomicBool::new(false),
            killed: AtomicBool::new(false),
        });
        let reader_state = Arc::clone(&state);
        let reader_wake = setup!(wake_read.try_clone());
        let reader = thread::Builder::new()
            .name("pty-reader".to_owned())
            .spawn(move || {
                reader_loop(master, reader_wake, &mut handler, reader_state);
            })
            .map_err(|error| {
                shutdown_child(process, Duration::from_secs(2));
                PtyError::Spawn(error)
            })?;
        let writer_state = Arc::clone(&state);
        let writer_wake = wake_read;
        let writer = match thread::Builder::new()
            .name("pty-writer".to_owned())
            .spawn(move || writer_loop(writer_file, writer_wake, receiver, writer_state))
        {
            Ok(handle) => handle,
            Err(error) => {
                shutdown_child(process, Duration::from_secs(2));
                wake(&wake_write);
                let _ = reader.join();
                return Err(PtyError::Spawn(error));
            }
        };
        Ok(Self {
            pid,
            state,
            pty: Mutex::new(Some(pty)),
            sender: Mutex::new(Some(sender)),
            reader: Mutex::new(Some(reader)),
            writer: Mutex::new(Some(writer)),
            wake: wake_write,
            kill_lock: Mutex::new(()),
        })
    }

    /// Queues bytes for the program without blocking.
    pub fn write(&self, data: &[u8]) -> Result<(), PtyError> {
        if self.has_exited() || self.state.killed.load(Ordering::Acquire) {
            return Err(PtyError::Closed);
        }
        let sender = self.sender.lock().unwrap_or_else(|e| e.into_inner());
        let Some(sender) = sender.as_ref() else {
            return Err(PtyError::Closed);
        };
        match sender.try_send(data.to_vec()) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => Err(PtyError::QueueFull),
            Err(mpsc::TrySendError::Disconnected(_)) => Err(PtyError::Closed),
        }
    }

    /// Changes the PTY size.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        if self.has_exited() || self.state.killed.load(Ordering::Acquire) {
            return Err(PtyError::Closed);
        }
        let mut guard = self.pty.lock().unwrap_or_else(|e| e.into_inner());
        let Some(pty) = guard.as_mut() else {
            return Err(PtyError::Closed);
        };
        pty.on_resize(WindowSize {
            num_lines: rows.max(1),
            num_cols: cols.max(1),
            cell_width: 1,
            cell_height: 1,
        });
        Ok(())
    }

    /// OS process id of the program.
    pub fn pid(&self) -> u32 {
        self.pid
    }
    /// Whether the reader has ended.
    pub fn has_exited(&self) -> bool {
        self.state.exited.load(Ordering::Acquire)
    }

    /// Terminates the program and joins both threads.
    pub fn kill(&self, grace: Duration) {
        let _guard = self.kill_lock.lock().unwrap_or_else(|e| e.into_inner());
        if self.state.killed.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(pid) = process_pid(self.pid) {
            signal_group(pid, Signal::HUP);
            if !wait_for_exit(pid, grace) {
                signal_group(pid, Signal::KILL);
                reap(pid);
            }
        }
        wake(&self.wake);
        self.pty.lock().unwrap_or_else(|e| e.into_inner()).take();
        self.sender.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(handle) = self.writer.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.reader.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = handle.join();
        }
    }
}
impl Drop for PtyProcess {
    fn drop(&mut self) {
        self.kill(Duration::from_secs(2));
    }
}

fn set_nonblocking(file: &File) -> std::io::Result<()> {
    let flags = fcntl_getfl(file).map_err(std::io::Error::from)?;
    fcntl_setfl(file, flags | OFlags::NONBLOCK).map_err(std::io::Error::from)
}
fn reader_loop(mut file: File, wake_fd: OwnedFd, handler: &mut dyn PtyHandler, state: Shared) {
    let mut buffer = vec![0; 64 * 1024];
    let mut stopping = false;
    loop {
        let mut fds = [
            PollFd::new(&file, PollFlags::IN),
            PollFd::new(&wake_fd, PollFlags::IN),
        ];
        if poll(&mut fds, None).is_err() {
            break;
        }
        if fds[1]
            .revents()
            .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR)
        {
            stopping = true;
        }
        loop {
            match file.read(&mut buffer) {
                Ok(0) => {
                    stopping = true;
                    break;
                }
                Ok(count) => handler.output(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    stopping = true;
                    break;
                }
            }
        }
        if stopping {
            break;
        }
    }
    state.exited.store(true, Ordering::Release);
    handler.exited();
}
fn writer_loop(mut file: File, wake_fd: OwnedFd, receiver: mpsc::Receiver<Vec<u8>>, state: Shared) {
    while let Ok(data) = receiver.recv() {
        if state.exited.load(Ordering::Acquire) || state.killed.load(Ordering::Acquire) {
            break;
        }
        let mut remaining = data.as_slice();
        while !remaining.is_empty() && !state.killed.load(Ordering::Acquire) {
            match file.write(remaining) {
                Ok(0) => return,
                Ok(count) => remaining = &remaining[count..],
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    let mut fds = [
                        PollFd::new(&file, PollFlags::OUT),
                        PollFd::new(&wake_fd, PollFlags::IN),
                    ];
                    if poll(&mut fds, None).is_err()
                        || fds[1]
                            .revents()
                            .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR)
                    {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    }
}
fn signal_group(pid: Pid, signal: Signal) {
    let _ = kill_process_group(pid, signal);
    let _ = kill_process(pid, signal);
}
fn wait_for_exit(pid: Pid, duration: Duration) -> bool {
    let deadline = Instant::now() + duration;
    loop {
        if reap_nonblocking(pid) {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        let left = deadline.saturating_duration_since(now);
        let timeout = rustix::event::Timespec {
            tv_sec: left.as_secs().try_into().unwrap_or(i64::MAX),
            tv_nsec: left.subsec_nanos().into(),
        };
        let mut fds: [PollFd<'_>; 0] = [];
        let _ = poll(&mut fds, Some(&timeout));
    }
}
fn reap_nonblocking(pid: Pid) -> bool {
    matches!(
        waitpid(Some(pid), WaitOptions::NOHANG),
        Ok(Some(_)) | Err(_)
    )
}
fn reap(pid: Pid) {
    loop {
        match waitpid(Some(pid), WaitOptions::empty()) {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) => {}
        }
    }
}
fn shutdown_child(pid: Pid, grace: Duration) {
    signal_group(pid, Signal::HUP);
    if !wait_for_exit(pid, grace) {
        signal_group(pid, Signal::KILL);
        reap(pid);
    }
}
fn wake(fd: &OwnedFd) {
    if let Ok(clone) = fd.try_clone() {
        let mut file = File::from(clone);
        let _ = file.write_all(&[1]);
    }
}
fn process_pid(pid: u32) -> Option<Pid> {
    i32::try_from(pid).ok().and_then(Pid::from_raw)
}
