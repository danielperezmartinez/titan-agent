use base64::Engine;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum WsError {
    Io(io::Error),
    Handshake(String),
    Protocol(String),
    Closed,
}
impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Handshake(s) => write!(f, "websocket handshake: {s}"),
            Self::Protocol(s) => write!(f, "websocket protocol: {s}"),
            Self::Closed => f.write_str("websocket closed"),
        }
    }
}
impl std::error::Error for WsError {}
impl From<io::Error> for WsError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub struct WsClient {
    stream: UnixStream,
    buffer: Vec<u8>,
    seed: u64,
}
impl WsClient {
    pub fn connect(path: &Path, timeout: Duration) -> Result<Self, WsError> {
        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
            ^ u64::from(std::process::id());
        let mut rng = seed;
        let mut key_bytes = [0; 16];
        for byte in &mut key_bytes {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            *byte = rng as u8;
        }
        let key = base64::engine::general_purpose::STANDARD.encode(key_bytes);
        write!(
            stream,
            "GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )?;
        let mut buffer = Vec::new();
        while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            let mut chunk = [0; 1024];
            let n = stream.read(&mut chunk)?;
            if n == 0 {
                return Err(WsError::Handshake("connection closed".into()));
            }
            buffer.extend_from_slice(&chunk[..n]);
            if buffer.len() > 16384 {
                return Err(WsError::Handshake("headers too large".into()));
            }
        }
        let end = buffer.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let header = String::from_utf8_lossy(&buffer[..end]);
        if !header
            .lines()
            .next()
            .is_some_and(|line| line.starts_with("HTTP/1.1 101"))
        {
            return Err(WsError::Handshake(
                header
                    .lines()
                    .next()
                    .unwrap_or("invalid response")
                    .to_owned(),
            ));
        }
        Ok(Self {
            stream,
            buffer: buffer[end..].to_vec(),
            seed,
        })
    }
    fn mask(&mut self, payload: &[u8], opcode: u8) -> Result<(), WsError> {
        let len = payload.len();
        let mut head = vec![0x80 | opcode];
        if len < 126 {
            head.push(0x80 | len as u8);
        } else if len <= 65535 {
            head.push(0x80 | 126);
            head.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            head.push(0x80 | 127);
            head.extend_from_slice(&(len as u64).to_be_bytes());
        }
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        let mask = self.seed.to_be_bytes();
        let mask = &mask[..4];
        head.extend_from_slice(mask);
        let mut bytes = Vec::with_capacity(len);
        bytes.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.stream.write_all(&head)?;
        self.stream.write_all(&bytes)?;
        Ok(())
    }
    pub fn send_text(&mut self, text: &str) -> Result<(), WsError> {
        self.mask(text.as_bytes(), 1)
    }
    fn exact(&mut self, size: usize, deadline: Instant) -> Result<Vec<u8>, WsError> {
        while self.buffer.len() < size {
            let remain = deadline.saturating_duration_since(Instant::now());
            if remain.is_zero() {
                return Err(WsError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "websocket receive timeout",
                )));
            }
            self.stream.set_read_timeout(Some(remain))?;
            let mut chunk = [0; 8192];
            let n = self.stream.read(&mut chunk)?;
            if n == 0 {
                return Err(WsError::Closed);
            }
            self.buffer.extend_from_slice(&chunk[..n]);
        }
        Ok(self.buffer.drain(..size).collect())
    }
    pub fn recv_text(&mut self, timeout: Duration) -> Result<String, WsError> {
        let deadline = Instant::now() + timeout;
        let mut message = Vec::new();
        let mut active = false;
        loop {
            let h = self.exact(2, deadline)?;
            let fin = h[0] & 0x80 != 0;
            let op = h[0] & 15;
            let masked = h[1] & 0x80 != 0;
            let mut len = (h[1] & 127) as u64;
            if len == 126 {
                len = u16::from_be_bytes(self.exact(2, deadline)?.try_into().unwrap()) as u64
            } else if len == 127 {
                len = u64::from_be_bytes(self.exact(8, deadline)?.try_into().unwrap())
            }
            if len > 16 * 1024 * 1024 {
                return Err(WsError::Protocol("message exceeds 16 MiB".into()));
            }
            let mask = if masked {
                Some(self.exact(4, deadline)?)
            } else {
                None
            };
            let mut p = self.exact(len as usize, deadline)?;
            if let Some(m) = mask {
                for (i, b) in p.iter_mut().enumerate() {
                    *b ^= m[i % 4]
                }
            }
            match op {
                8 => return Err(WsError::Closed),
                9 => self.mask(&p, 10)?,
                10 | 2 => {}
                1 => {
                    message = p;
                    active = true;
                    if fin {
                        return String::from_utf8(message)
                            .map_err(|e| WsError::Protocol(e.to_string()));
                    }
                }
                0 if active => {
                    message.extend(p);
                    if message.len() > 16 * 1024 * 1024 {
                        return Err(WsError::Protocol("message exceeds 16 MiB".into()));
                    }
                    if fin {
                        return String::from_utf8(message)
                            .map_err(|e| WsError::Protocol(e.to_string()));
                    }
                }
                _ => return Err(WsError::Protocol("invalid frame sequence".into())),
            }
        }
    }
}
