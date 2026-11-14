use crate::session::{AgentCli, SessionState};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::BufRead;
pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub root: String,
    pub name: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopAction {
    Stop,
    Interrupt,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub cli: AgentCli,
    pub name: String,
    pub cwd: String,
    pub project: Project,
    pub state: SessionState,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub attach: Option<Vec<String>>,
    #[serde(default)]
    pub stop: Option<StopAction>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfo {
    pub name: String,
    pub version: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientRequest {
    Hello {
        protocol: u32,
        client: PeerInfo,
    },
    SessionsList,
    SessionsSubscribe,
    TerminalOpen {
        session: String,
        cols: u16,
        rows: u16,
    },
    TerminalInput {
        session: String,
        data: Vec<u8>,
    },
    TerminalResize {
        session: String,
        cols: u16,
        rows: u16,
    },
    TerminalClose {
        session: String,
    },
    SessionStop {
        session: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonNotification {
    SessionsChanged { sessions: Vec<Session> },
    TerminalOutput { session: String, data: Vec<u8> },
    TerminalExited { session: String, code: Option<i32> },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Request {
        id: u64,
        request: ClientRequest,
    },
    Response {
        id: u64,
        result: Result<Value, RpcError>,
    },
    Notification(DaemonNotification),
}
pub mod error_code {
    pub const UNSUPPORTED_PROTOCOL: i32 = 1;
    pub const HELLO_REQUIRED: i32 = 2;
    pub const UNKNOWN_SESSION: i32 = 3;
    pub const NOT_AVAILABLE: i32 = 4;
    pub const INVALID_PARAMS: i32 = 5;
    pub const UNKNOWN_METHOD: i32 = 6;
    pub const INTERNAL: i32 = 7;
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloResult {
    pub protocol: u32,
    pub daemon: PeerInfo,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionsResult {
    pub sessions: Vec<Session>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    TooLong,
    InvalidJson(String),
    UnknownMethod { id: Option<u64>, method: String },
    InvalidParams { id: Option<u64>, message: String },
}
fn bad(id: Option<u64>) -> DecodeError {
    DecodeError::InvalidParams {
        id,
        message: "invalid parameters".into(),
    }
}
pub fn encode_line(m: &Message) -> String {
    let v = match m {
        Message::Request { id, request } => {
            let (method, p) = match request {
                ClientRequest::Hello { protocol, client } => {
                    ("hello", json!({"protocol":protocol,"client":client}))
                }
                ClientRequest::SessionsList => ("sessions/list", json!({})),
                ClientRequest::SessionsSubscribe => ("sessions/subscribe", json!({})),
                ClientRequest::TerminalOpen {
                    session,
                    cols,
                    rows,
                } => (
                    "terminal/open",
                    json!({"session":session,"cols":cols,"rows":rows}),
                ),
                ClientRequest::TerminalInput { session, data } => (
                    "terminal/input",
                    json!({"session":session,"data":STANDARD.encode(data)}),
                ),
                ClientRequest::TerminalResize {
                    session,
                    cols,
                    rows,
                } => (
                    "terminal/resize",
                    json!({"session":session,"cols":cols,"rows":rows}),
                ),
                ClientRequest::TerminalClose { session } => {
                    ("terminal/close", json!({"session":session}))
                }
                ClientRequest::SessionStop { session } => {
                    ("session/stop", json!({"session":session}))
                }
            };
            json!({"id":id,"method":method,"params":p})
        }
        Message::Response { id, result } => match result {
            Ok(v) => json!({"id":id,"result":v}),
            Err(e) => json!({"id":id,"error":e}),
        },
        Message::Notification(n) => match n {
            DaemonNotification::SessionsChanged { sessions } => {
                json!({"method":"sessions/changed","params":{"sessions":sessions}})
            }
            DaemonNotification::TerminalOutput { session, data } => {
                json!({"method":"terminal/output","params":{"session":session,"data":STANDARD.encode(data)}})
            }
            DaemonNotification::TerminalExited { session, code } => {
                json!({"method":"terminal/exited","params":{"session":session,"code":code}})
            }
        },
    };
    format!("{}\n", v)
}
pub fn decode_line(line: &str) -> Result<Message, DecodeError> {
    let l = line
        .strip_suffix("\r\n")
        .or_else(|| line.strip_suffix('\n'))
        .unwrap_or(line);
    if l.len() > MAX_LINE_BYTES {
        return Err(DecodeError::TooLong);
    }
    let v: Value = serde_json::from_str(l).map_err(|e| DecodeError::InvalidJson(e.to_string()))?;
    let Some(o) = v.as_object() else {
        return Err(DecodeError::InvalidJson("not object".into()));
    };
    let id = o.get("id").and_then(Value::as_u64);
    let method = o.get("method").and_then(Value::as_str);
    if let Some(method) = method {
        let p = o.get("params").cloned().unwrap_or(json!({}));
        if id.is_none() {
            let notification = match method {
                "sessions/changed" => serde_json::from_value::<structs::Changed>(p).map(|x| {
                    Message::Notification(DaemonNotification::SessionsChanged {
                        sessions: x.sessions,
                    })
                }),
                "terminal/output" => serde_json::from_value::<structs::Data>(p).and_then(|x| {
                    STANDARD
                        .decode(x.data)
                        .map(|data| {
                            Message::Notification(DaemonNotification::TerminalOutput {
                                session: x.session,
                                data,
                            })
                        })
                        .map_err(serde::de::Error::custom)
                }),
                "terminal/exited" => serde_json::from_value::<structs::Exited>(p).map(|x| {
                    Message::Notification(DaemonNotification::TerminalExited {
                        session: x.session,
                        code: x.code,
                    })
                }),
                _ => {
                    return Err(DecodeError::UnknownMethod {
                        id,
                        method: method.into(),
                    });
                }
            };
            return notification.map_err(|_| bad(id));
        }
        if matches!(
            method,
            "sessions/changed" | "terminal/output" | "terminal/exited"
        ) {
            return Err(DecodeError::UnknownMethod {
                id,
                method: method.into(),
            });
        }
        let req = match method {
            "hello" => serde_json::from_value::<structs::Hello>(p).map(|x| ClientRequest::Hello {
                protocol: x.protocol,
                client: x.client,
            }),
            "sessions/list" | "sessions/subscribe" => {
                if !p.is_object() {
                    return Err(bad(id));
                }
                if method == "sessions/list" {
                    Ok(ClientRequest::SessionsList)
                } else {
                    Ok(ClientRequest::SessionsSubscribe)
                }
            }
            "terminal/open" => {
                serde_json::from_value::<structs::Open>(p).map(|x| ClientRequest::TerminalOpen {
                    session: x.session,
                    cols: x.cols,
                    rows: x.rows,
                })
            }
            "terminal/input" => serde_json::from_value::<structs::Data>(p).and_then(|x| {
                STANDARD
                    .decode(x.data)
                    .map(|data| ClientRequest::TerminalInput {
                        session: x.session,
                        data,
                    })
                    .map_err(serde::de::Error::custom)
            }),
            "terminal/resize" => {
                serde_json::from_value::<structs::Open>(p).map(|x| ClientRequest::TerminalResize {
                    session: x.session,
                    cols: x.cols,
                    rows: x.rows,
                })
            }
            "terminal/close" => serde_json::from_value::<structs::Sess>(p)
                .map(|x| ClientRequest::TerminalClose { session: x.session }),
            "session/stop" => serde_json::from_value::<structs::Sess>(p)
                .map(|x| ClientRequest::SessionStop { session: x.session }),
            _ => {
                return Err(DecodeError::UnknownMethod {
                    id,
                    method: method.into(),
                });
            }
        };
        return match req {
            Ok(r) if id.is_some() => Ok(Message::Request {
                id: id.unwrap(),
                request: r,
            }),
            Ok(_) => Err(DecodeError::UnknownMethod {
                id,
                method: method.into(),
            }),
            Err(_) => Err(bad(id)),
        };
    }
    if let Some(id) = id {
        if let Some(r) = o.get("result") {
            return Ok(Message::Response {
                id,
                result: Ok(r.clone()),
            });
        }
        if let Some(e) = o.get("error") {
            return serde_json::from_value(e.clone())
                .map(|e| Message::Response { id, result: Err(e) })
                .map_err(|x| DecodeError::InvalidJson(x.to_string()));
        }
    }
    Err(DecodeError::InvalidJson("invalid message".into()))
}
mod structs {
    use super::*;
    #[derive(Deserialize)]
    pub struct Hello {
        pub protocol: u32,
        pub client: PeerInfo,
    }
    #[derive(Deserialize)]
    pub struct Open {
        pub session: String,
        pub cols: u16,
        pub rows: u16,
    }
    #[derive(Deserialize)]
    pub struct Sess {
        pub session: String,
    }
    #[derive(Deserialize)]
    pub struct Data {
        pub session: String,
        pub data: String,
    }
    #[derive(Deserialize)]
    pub struct Changed {
        pub sessions: Vec<Session>,
    }
    #[derive(Deserialize)]
    pub struct Exited {
        pub session: String,
        pub code: Option<i32>,
    }
}
#[derive(Debug)]
pub enum LineError {
    TooLong,
    InvalidUtf8,
    Io(std::io::Error),
}
pub struct LineReader<R: BufRead> {
    r: R,
}
impl<R: BufRead> LineReader<R> {
    pub fn new(reader: R) -> Self {
        Self { r: reader }
    }
    pub fn next_line(&mut self) -> Result<Option<String>, LineError> {
        let mut b = Vec::new();
        loop {
            let x = self.r.fill_buf().map_err(LineError::Io)?;
            if x.is_empty() {
                if b.is_empty() {
                    return Ok(None);
                }
                break;
            }
            let n = x
                .iter()
                .position(|&c| c == b'\n')
                .map_or(x.len(), |i| i + 1);
            if b.len() + n > MAX_LINE_BYTES + 2 {
                self.r.consume(n);
                return Err(LineError::TooLong);
            }
            let end = n;
            b.extend_from_slice(&x[..end]);
            self.r.consume(n);
            if b.last() == Some(&b'\n') {
                break;
            }
        }
        if b.last() == Some(&b'\n') {
            b.pop();
            if b.last() == Some(&b'\r') {
                b.pop();
            }
        }
        if b.len() > MAX_LINE_BYTES {
            return Err(LineError::TooLong);
        }
        String::from_utf8(b)
            .map(Some)
            .map_err(|_| LineError::InvalidUtf8)
    }
}
