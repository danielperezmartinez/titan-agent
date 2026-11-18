use titan_agent::conversation::{SessionBody, pick_task, settle, toggle_monitor};
use titan_agent_core::monitor::TaskId;

#[test]
fn session_monitor_toggle_selection_and_snapshot_settlement_follow_open_tasks() {
    let latest = TaskId {
        folder: "/fixture/team".into(),
        task: "latest".into(),
    };
    let older = TaskId {
        folder: latest.folder.clone(),
        task: "older".into(),
    };
    let foreign = TaskId {
        folder: "/fixture/other-team".into(),
        task: older.task.clone(),
    };
    let tasks = vec![latest.clone(), older.clone()];
    let terminal = SessionBody::Terminal;
    let monitor = SessionBody::Monitor {
        task: latest.clone(),
    };
    let selected = SessionBody::Monitor {
        task: older.clone(),
    };
    assert_eq!(toggle_monitor(&terminal, &tasks), monitor);
    assert_eq!(toggle_monitor(&terminal, &[]), terminal);
    assert_eq!(toggle_monitor(&monitor, &tasks), terminal);
    assert_eq!(toggle_monitor(&monitor, &[]), terminal);
    assert_eq!(pick_task(&terminal, &older, &tasks), selected);
    assert_eq!(pick_task(&monitor, &older, &tasks), selected);
    assert_eq!(pick_task(&selected, &foreign, &tasks), selected);
    assert_eq!(pick_task(&terminal, &foreign, &tasks), terminal);
    assert_eq!(settle(&terminal, &tasks), terminal);
    assert_eq!(settle(&selected, &tasks), selected);
    assert_eq!(settle(&selected, &[latest]), monitor);
    assert_eq!(settle(&selected, &[]), terminal);
}
