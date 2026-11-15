use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub const TIMEOUT: Duration = Duration::from_secs(2);
static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct TempDir(pub PathBuf);
impl TempDir {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ta-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn server(
    action: impl FnOnce(UnixStream) + Send + 'static,
) -> (TempDir, PathBuf, JoinHandle<()>) {
    let dir = TempDir::new();
    let path = dir.path().join("s");
    let listener = UnixListener::bind(&path).unwrap();
    let handle = thread::spawn(move || {
        // A broken client must fail the test without leaving accept blocked forever.
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + TIMEOUT;
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "client did not connect"
                    );
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("accept: {error}"),
            }
        };
        stream.set_read_timeout(Some(TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(TIMEOUT)).unwrap();
        action(stream);
    });
    (dir, path, handle)
}

pub fn read_upgrade(stream: &mut UnixStream) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
        assert!(bytes.len() < 16384);
    }
    String::from_utf8(bytes).unwrap()
}

pub fn upgrade(stream: &mut UnixStream) {
    read_upgrade(stream);
    stream.write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n").unwrap();
}

pub fn frame(fin: bool, opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![if fin { 0x80 | opcode } else { opcode }];
    match payload.len() {
        0..=125 => bytes.push(payload.len() as u8),
        126..=65535 => {
            bytes.push(126);
            bytes.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        _ => {
            bytes.push(127);
            bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
    }
    bytes.extend_from_slice(payload);
    bytes
}

pub fn read_client_frame(stream: &mut UnixStream) -> (bool, u8, Vec<u8>) {
    let mut head = [0; 2];
    stream.read_exact(&mut head).unwrap();
    assert_ne!(head[1] & 0x80, 0, "client frames must be masked");
    let length = match head[1] & 127 {
        126 => {
            let mut bytes = [0; 2];
            stream.read_exact(&mut bytes).unwrap();
            u16::from_be_bytes(bytes) as usize
        }
        127 => {
            let mut bytes = [0; 8];
            stream.read_exact(&mut bytes).unwrap();
            usize::try_from(u64::from_be_bytes(bytes)).unwrap()
        }
        length => usize::from(length),
    };
    assert!(length <= 17 * 1024 * 1024);
    let mut mask = [0; 4];
    stream.read_exact(&mut mask).unwrap();
    let mut payload = vec![0; length];
    stream.read_exact(&mut payload).unwrap();
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[index % 4];
    }
    (head[0] & 0x80 != 0, head[0] & 15, payload)
}
