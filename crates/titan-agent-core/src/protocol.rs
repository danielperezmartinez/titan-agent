use crate::session::{Session, SessionKey};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};
use std::fmt;
use std::io::{self, BufRead};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub type TerminalId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bytes(pub Vec<u8>);
impl Serialize for Bytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(&self.0))
    }
}
impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        STANDARD
            .decode(value)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}
impl From<Vec<u8>> for Bytes {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}
impl From<&[u8]> for Bytes {
    fn from(value: &[u8]) -> Self {
        Self(value.to_vec())
    }
}
impl From<&str> for Bytes {
    fn from(value: &str) -> Self {
        Self(value.as_bytes().to_vec())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientMessage {
    pub id: u64,
    pub request: Request,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Request {
    Hello {
        protocol: u32,
        client: String,
    },
    Subscribe,
    OpenTerminal {
        session: SessionKey,
        cols: u16,
        rows: u16,
    },
    Input {
        terminal: TerminalId,
        data: Bytes,
    },
    Resize {
        terminal: TerminalId,
        cols: u16,
        rows: u16,
    },
    Detach {
        terminal: TerminalId,
    },
    CloseTerminal {
        terminal: TerminalId,
    },
    StopSession {
        session: SessionKey,
    },
    Shutdown,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Response { id: u64, response: Response },
    Error { id: u64, error: ErrorBody },
    Event { event: Event },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    Welcome {
        protocol: u32,
        daemon: String,
    },
    Sessions {
        sessions: Vec<Session>,
    },
    TerminalOpened {
        terminal: TerminalId,
        session: SessionKey,
        cols: u16,
        rows: u16,
        reused: bool,
    },
    Done,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Sessions {
        sessions: Vec<Session>,
    },
    TerminalSnapshot {
        terminal: TerminalId,
        cols: u16,
        rows: u16,
        data: Bytes,
    },
    TerminalOutput {
        terminal: TerminalId,
        data: Bytes,
    },
    TerminalExited {
        terminal: TerminalId,
        code: Option<i32>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ProtocolMismatch,
    NotGreeted,
    BadRequest,
    UnknownSession,
    NotAttachable,
    UnknownTerminal,
    SpawnFailed,
    StopFailed,
    Unsupported,
    Internal,
}

pub fn encode_line<T: Serialize>(message: &T) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(message).expect("serializing protocol message cannot fail");
    bytes.push(b'\n');
    bytes
}
pub fn decode_line<T: DeserializeOwned>(line: &[u8]) -> Result<T, FrameError> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    serde_json::from_slice(line).map_err(|error| FrameError::Json(error.to_string()))
}

pub fn read_frame<R: BufRead, T: DeserializeOwned>(
    reader: &mut R,
) -> Result<Option<T>, FrameError> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if frame.is_empty() {
                return Ok(None);
            }
            return Err(FrameError::UnexpectedEof);
        }
        let take = available
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        let has_newline = available[take - 1] == b'\n';
        let content_len = take - usize::from(has_newline);
        if frame.len() + content_len > MAX_FRAME_BYTES {
            return Err(FrameError::TooLarge);
        }
        frame.extend_from_slice(&available[..take]);
        reader.consume(take);
        if has_newline {
            let payload = &frame[..frame.len() - 1];
            if payload.iter().all(u8::is_ascii_whitespace) {
                frame.clear();
                continue;
            }
            return decode_line(payload).map(Some);
        }
    }
}

#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    TooLarge,
    UnexpectedEof,
    Json(String),
}
impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::TooLarge => formatter.write_str("frame too large"),
            Self::UnexpectedEof => formatter.write_str("unexpected end of frame"),
            Self::Json(error) => write!(formatter, "invalid frame JSON: {error}"),
        }
    }
}
impl std::error::Error for FrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Io(error) = self {
            Some(error)
        } else {
            None
        }
    }
}
impl From<io::Error> for FrameError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
