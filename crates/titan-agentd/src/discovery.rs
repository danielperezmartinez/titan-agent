use serde_json::{Value, json};
use std::io::{BufRead, Write};

#[derive(Debug)]
pub enum CodexError {
    Io(std::io::Error),
    Rpc { code: i64, message: String },
    Protocol(String),
    Closed,
}
impl From<std::io::Error> for CodexError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

pub struct CodexClient<R: BufRead, W: Write> {
    reader: R,
    writer: W,
    next_id: u64,
}
impl<R: BufRead, W: Write> CodexClient<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            next_id: 0,
        }
    }
    fn request(&mut self, method: &str, params: Value) -> Result<Value, CodexError> {
        self.next_id += 1;
        let id = self.next_id;
        writeln!(
            self.writer,
            "{}",
            json!({"id":id,"method":method,"params":params})
        )?;
        self.writer.flush()?;
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line)? == 0 {
                return Err(CodexError::Closed);
            }
            let value: Value =
                serde_json::from_str(&line).map_err(|e| CodexError::Protocol(e.to_string()))?;
            if value.get("method").is_some() {
                continue;
            }
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(CodexError::Rpc {
                    code: error["code"].as_i64().unwrap_or(0),
                    message: error["message"].as_str().unwrap_or("RPC error").into(),
                });
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| CodexError::Protocol("missing result".into()));
        }
    }
    pub fn initialize(&mut self, version: &str) -> Result<(), CodexError> {
        self.request(
            "initialize",
            json!({"clientInfo":{"name":"titan-agentd","version":version}}),
        )?;
        writeln!(self.writer, "{}", json!({"method":"initialized"}))?;
        self.writer.flush()?;
        Ok(())
    }
    pub fn loaded_threads(&mut self) -> Result<Vec<Value>, CodexError> {
        let mut cursor = Value::Null;
        let mut ids = Vec::new();
        loop {
            let mut params = json!({});
            if !cursor.is_null() {
                params["cursor"] = cursor.clone();
            }
            let result = self.request("thread/loaded/list", params)?;
            if let Some(data) = result.get("data").and_then(Value::as_array) {
                ids.extend(data.iter().filter_map(|v| v.as_str().map(str::to_owned)));
            }
            cursor = result.get("nextCursor").cloned().unwrap_or(Value::Null);
            if cursor.is_null() {
                break;
            }
        }
        ids.into_iter()
            .map(|id| {
                self.request("thread/read", json!({"threadId":id,"includeTurns":false}))
                    .map(|v| v["thread"].clone())
            })
            .collect()
    }
    pub fn interrupt(&mut self, thread_id: &str) -> Result<bool, CodexError> {
        let result = self.request(
            "thread/turns/list",
            json!({"threadId":thread_id,"limit":5,"itemsView":"notLoaded"}),
        )?;
        if let Some(turn) = result
            .get("data")
            .and_then(Value::as_array)
            .and_then(|a| a.iter().find(|t| t["status"] == "inProgress"))
        {
            self.request(
                "turn/interrupt",
                json!({"threadId":thread_id,"turnId":turn["id"]}),
            )?;
            return Ok(true);
        }
        Ok(false)
    }
}
