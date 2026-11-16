//! Read-only transcript of a Codex thread, as the daemon serves it to the
//! interface (request `transcript`). It is built from the official app-server
//! answers (`thread/items/list`, or `thread/read` with its turns), never from
//! Codex's own files, and reading it opens no writer on the thread.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Items the daemon asks for at once (the most recent ones).
pub const TRANSCRIPT_ITEMS: usize = 200;
/// Characters (not bytes) kept of an item's text.
pub const TRANSCRIPT_TEXT_MAX: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptKind {
    User,
    Agent,
    Reasoning,
    Command,
    FileChange,
    Tool,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptItem {
    /// Id of the Codex turn the item belongs to.
    pub turn: String,
    pub kind: TranscriptKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A page of `thread/items/list` asked with `sortDirection: "desc"` (newest
/// first). Returns the items oldest first, and whether older ones remain.
pub fn transcript_from_items_page(page: &Value) -> (Vec<TranscriptItem>, bool) {
    let mut items: Vec<TranscriptItem> = page
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let turn = entry.get("turnId").and_then(Value::as_str).unwrap_or("");
            convert(turn, entry.get("item")?)
        })
        .collect();
    items.reverse();
    let more = page
        .get("nextCursor")
        .is_some_and(|cursor| !cursor.is_null());
    (items, more)
}

/// The answer of `thread/read` with `includeTurns: true` (turns oldest
/// first). Returns its last [`TRANSCRIPT_ITEMS`] items oldest first, and
/// whether older ones were left out.
pub fn transcript_from_thread(read: &Value) -> (Vec<TranscriptItem>, bool) {
    let mut items: Vec<TranscriptItem> = read
        .get("thread")
        .and_then(|thread| thread.get("turns"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|turn| {
            let id = turn.get("id").and_then(Value::as_str).unwrap_or("");
            turn.get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(move |item| convert(id, item))
        })
        .collect();
    let extra = items.len().saturating_sub(TRANSCRIPT_ITEMS);
    items.drain(..extra);
    (items, extra > 0)
}

/// One Codex thread item as a transcript item; `None` when it has no type or
/// nothing to show.
fn convert(turn: &str, item: &Value) -> Option<TranscriptItem> {
    let kind_name = item.get("type").and_then(Value::as_str)?;
    let str_of = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or("");
    let strings = |key: &str, field: Option<&str>, separator: &str| -> String {
        item.get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| match field {
                Some(field) => entry.get(field).and_then(Value::as_str),
                None => entry.as_str(),
            })
            .collect::<Vec<_>>()
            .join(separator)
    };
    let mut detail = None;
    let (kind, text) = match kind_name {
        "userMessage" => {
            let text = item
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|entry| entry.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            (TranscriptKind::User, text)
        }
        "agentMessage" => (TranscriptKind::Agent, str_of("text").to_string()),
        "reasoning" => (TranscriptKind::Reasoning, strings("summary", None, "\n")),
        "commandExecution" => {
            detail = item
                .get("exitCode")
                .and_then(Value::as_i64)
                .map(|code| format!("código {code}"));
            (TranscriptKind::Command, str_of("command").to_string())
        }
        "fileChange" => (
            TranscriptKind::FileChange,
            strings("changes", Some("path"), ", "),
        ),
        "mcpToolCall" => (
            TranscriptKind::Tool,
            format!("{}.{}", str_of("server"), str_of("tool")),
        ),
        "dynamicToolCall" => (TranscriptKind::Tool, str_of("tool").to_string()),
        "webSearch" => (
            TranscriptKind::Tool,
            format!("búsqueda: {}", str_of("query")),
        ),
        "plan" => (TranscriptKind::Other, str_of("text").to_string()),
        other => (TranscriptKind::Other, other.to_string()),
    };
    if text.is_empty() {
        return None;
    }
    Some(TranscriptItem {
        turn: turn.to_string(),
        kind,
        text: cap(text),
        detail,
    })
}

fn cap(text: String) -> String {
    match text.char_indices().nth(TRANSCRIPT_TEXT_MAX) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn items_page_is_returned_oldest_first() {
        let page = json!({
            "data": [
                {"turnId": "t2", "item": {"type": "agentMessage", "text": "hecho"}},
                {"turnId": "t2", "item": {"type": "commandExecution", "command": "cargo test", "exitCode": 101}},
                {"turnId": "t1", "item": {"type": "reasoning", "summary": [], "content": ["oculto"]}},
                {"turnId": "t1", "item": {"type": "userMessage", "content": [
                    {"type": "text", "text": "hola"}, {"type": "image", "url": "x"}, {"type": "text", "text": "y"}]}},
                {"turnId": "t1", "nothing": true},
            ],
            "nextCursor": null,
        });
        let (items, more) = transcript_from_items_page(&page);
        assert!(!more);
        let kinds: Vec<_> = items.iter().map(|i| (i.turn.as_str(), i.kind)).collect();
        assert_eq!(
            kinds,
            [
                ("t1", TranscriptKind::User),
                ("t2", TranscriptKind::Command),
                ("t2", TranscriptKind::Agent)
            ]
        );
        assert_eq!(items[0].text, "hola\ny");
        assert_eq!(items[1].detail.as_deref(), Some("código 101"));
    }

    #[test]
    fn tools_files_and_unknown_types() {
        let thread = json!({"thread": {"turns": [{"id": "t1", "items": [
            {"type": "fileChange", "changes": [{"path": "a.rs", "kind": {"type": "add"}}, {"path": "b.rs"}]},
            {"type": "mcpToolCall", "server": "docs", "tool": "search"},
            {"type": "dynamicToolCall", "tool": "probe"},
            {"type": "webSearch", "query": "gpui ime"},
            {"type": "plan", "text": "1. leer"},
            {"type": "contextCompaction"},
        ]}]}});
        let (items, more) = transcript_from_thread(&thread);
        assert!(!more);
        let texts: Vec<_> = items.iter().map(|i| (i.kind, i.text.as_str())).collect();
        assert_eq!(
            texts,
            [
                (TranscriptKind::FileChange, "a.rs, b.rs"),
                (TranscriptKind::Tool, "docs.search"),
                (TranscriptKind::Tool, "probe"),
                (TranscriptKind::Tool, "búsqueda: gpui ime"),
                (TranscriptKind::Other, "1. leer"),
                (TranscriptKind::Other, "contextCompaction"),
            ]
        );
    }

    #[test]
    fn long_texts_are_cut_by_characters_and_old_items_dropped() {
        let long = "é".repeat(TRANSCRIPT_TEXT_MAX + 5);
        let (items, _) = transcript_from_items_page(&json!({"data": [
            {"turnId": "t", "item": {"type": "agentMessage", "text": long}}]}));
        assert_eq!(items[0].text.chars().count(), TRANSCRIPT_TEXT_MAX + 1);
        assert!(items[0].text.ends_with('…'));

        let many: Vec<Value> = (0..TRANSCRIPT_ITEMS + 3)
            .map(|n| json!({"type": "agentMessage", "text": n.to_string()}))
            .collect();
        let (items, more) =
            transcript_from_thread(&json!({"thread": {"turns": [{"id": "t", "items": many}]}}));
        assert!(more);
        assert_eq!(items.len(), TRANSCRIPT_ITEMS);
        assert_eq!(items[0].text, "3");
    }
}
