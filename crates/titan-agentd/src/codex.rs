use crate::ws::{WsClient, WsError};
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::Instant;

pub fn control_socket(codex_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    let root = codex_home
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h).join(".codex")))?;
    Some(root.join("app-server-control/app-server-control.sock"))
}
pub struct CodexRpc {
    ws: WsClient,
    next_id: u64,
}
enum ResponseError {
    Rpc(String),
    Ws(WsError),
}
impl From<WsError> for ResponseError {
    fn from(error: WsError) -> Self {
        Self::Ws(error)
    }
}
impl CodexRpc {
    pub fn connect(path: &Path, timeout: Duration) -> Result<Self, WsError> {
        let mut rpc = Self {
            ws: WsClient::connect(path, timeout)?,
            next_id: 1,
        };
        let init =
            json!({"clientInfo":{"name":"titan-agentd","version":env!("CARGO_PKG_VERSION")}});
        let deadline = Instant::now() + timeout;
        rpc.send(1, "initialize", init, deadline)?;
        rpc.response(1, deadline).map_err(ResponseError::into_ws)?;
        rpc.ws
            .send_text_until(&json!({"method":"initialized"}).to_string(), deadline)?;
        rpc.next_id = 2;
        Ok(rpc)
    }
    fn send(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<(), WsError> {
        self.ws.send_text_until(
            &json!({"id":id,"method":method,"params":params}).to_string(),
            deadline,
        )
    }
    fn response(&mut self, id: u64, deadline: Instant) -> Result<Value, ResponseError> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ResponseError::Ws(WsError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "RPC timeout",
                ))));
            }
            let msg: Value = serde_json::from_str(&self.ws.recv_text(remaining)?)
                .map_err(|e| ResponseError::Ws(WsError::Protocol(e.to_string())))?;
            if msg.get("method").is_some() {
                continue;
            }
            if msg.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = msg.get("error") {
                return Err(ResponseError::Rpc(
                    error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("RPC error")
                        .to_owned(),
                ));
            }
            return msg
                .get("result")
                .cloned()
                .ok_or_else(|| ResponseError::Ws(WsError::Protocol("missing result".into())));
        }
    }
    pub fn call(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, WsError> {
        let id = self.next_id;
        self.next_id += 1;
        let deadline = Instant::now() + timeout;
        self.send(id, method, params, deadline)?;
        self.response(id, deadline).map_err(ResponseError::into_ws)
    }
    pub fn loaded_threads(&mut self, timeout: Duration) -> Result<Vec<Value>, WsError> {
        let mut cursor: Option<String> = None;
        let mut ids = Vec::new();
        loop {
            let params = cursor
                .as_ref()
                .map(|c| json!({"cursor":c}))
                .unwrap_or_else(|| json!({}));
            let page = self.call("thread/loaded/list", params, timeout)?;
            ids.extend(
                page.get("data")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
            cursor = page
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        let mut threads = Vec::new();
        for id in ids {
            let request_id = self.next_id;
            self.next_id += 1;
            let deadline = Instant::now() + timeout;
            self.send(
                request_id,
                "thread/read",
                json!({"threadId":id,"includeTurns":false}),
                deadline,
            )?;
            match self.response(request_id, deadline) {
                Ok(result) => {
                    if let Some(thread) = result.get("thread") {
                        threads.push(thread.clone());
                    }
                }
                Err(ResponseError::Rpc(_)) => {}
                Err(ResponseError::Ws(error)) => return Err(error),
            }
        }
        Ok(threads)
    }
}
impl ResponseError {
    fn into_ws(self) -> WsError {
        match self {
            Self::Rpc(message) => WsError::Protocol(message),
            Self::Ws(error) => error,
        }
    }
}
