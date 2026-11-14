use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    thread,
    time::Duration,
};
use titan_agentd::discovery::{CodexClient, CodexError};

fn pair() -> (CodexClient<BufReader<UnixStream>, UnixStream>, UnixStream) {
    let (client, server) = UnixStream::pair().unwrap();
    for stream in [&client, &server] {
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
    }
    (
        CodexClient::new(BufReader::new(client.try_clone().unwrap()), client),
        server,
    )
}

fn receive(reader: &mut BufReader<UnixStream>, id: u64, method: &str) -> Value {
    let mut line = String::new();
    assert!(reader.read_line(&mut line).unwrap() > 0);
    let request: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(request["id"], id);
    assert_eq!(request["method"], method);
    assert!(request.get("jsonrpc").is_none());
    request["params"].clone()
}

fn send(stream: &mut UnixStream, value: Value) {
    writeln!(stream, "{value}").unwrap();
}

#[test]
fn initialization_pagination_reads_and_ignored_server_messages() {
    let (mut client, mut server) = pair();
    let worker = thread::spawn(move || {
        let mut reader = BufReader::new(server.try_clone().unwrap());
        let params = receive(&mut reader, 1, "initialize");
        assert_eq!(params["clientInfo"]["name"], "titan-agentd");
        assert_eq!(params["clientInfo"]["version"], "fixture-version");
        send(&mut server, json!({"method":"fixture/notice","params":{}}));
        send(
            &mut server,
            json!({"id":900,"method":"fixture/approval","params":{}}),
        );
        send(&mut server, json!({"id":1,"result":{}}));
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap(),
            json!({"method":"initialized"})
        );
        receive(&mut reader, 2, "thread/loaded/list");
        send(
            &mut server,
            json!({"id":2,"result":{"data":["thread-a"],"nextCursor":"page-b"}}),
        );
        // The contract does not fix whether reads precede the next page fetch.
        let mut saw_page = false;
        let mut read_ids = Vec::new();
        for id in 3..=5 {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["id"], id);
            assert!(request.get("jsonrpc").is_none());
            match request["method"].as_str().unwrap() {
                "thread/loaded/list" => {
                    assert!(!saw_page);
                    assert_eq!(request["params"]["cursor"], "page-b");
                    saw_page = true;
                    send(
                        &mut server,
                        json!({"id":id,"result":{"data":["thread-b"],"nextCursor":null}}),
                    );
                }
                "thread/read" => {
                    let thread = request["params"]["threadId"].as_str().unwrap();
                    assert_eq!(request["params"]["includeTurns"], false);
                    assert!(thread == "thread-a" || (saw_page && thread == "thread-b"));
                    read_ids.push(thread.to_owned());
                    send(&mut server, json!({"method":"fixture/progress"}));
                    send(
                        &mut server,
                        json!({"id":id,"result":{"thread":{"id":thread,"fixture":true}}}),
                    );
                }
                method => panic!("unexpected request: {method}"),
            }
        }
        assert!(saw_page);
        assert_eq!(read_ids, ["thread-a", "thread-b"]);
    });
    client.initialize("fixture-version").unwrap();
    assert_eq!(
        client.loaded_threads().unwrap(),
        vec![
            json!({"id":"thread-a","fixture":true}),
            json!({"id":"thread-b","fixture":true})
        ]
    );
    worker.join().unwrap();
}

#[test]
fn interrupt_targets_first_in_progress_turn_and_returns_false_when_idle() {
    let (mut client, mut server) = pair();
    let worker = thread::spawn(move || {
        let mut reader = BufReader::new(server.try_clone().unwrap());
        for (id, name) in [(1, "busy-thread"), (3, "idle-thread")] {
            let params = receive(&mut reader, id, "thread/turns/list");
            assert_eq!(
                params,
                json!({"threadId":name,"limit":5,"itemsView":"notLoaded"})
            );
            let turns = if id == 1 {
                json!([
                    {"id":"finished","status":"completed"},
                    {"id":"active-first","status":"inProgress"},
                    {"id":"active-second","status":"inProgress"}
                ])
            } else {
                json!([{"id":"done","status":"completed"}])
            };
            send(
                &mut server,
                json!({"id":id,"result":{"data":turns,"nextCursor":null}}),
            );
            if id == 1 {
                assert_eq!(
                    receive(&mut reader, 2, "turn/interrupt"),
                    json!({"threadId":name,"turnId":"active-first"})
                );
                send(&mut server, json!({"id":2,"result":{}}));
            }
        }
    });
    assert!(client.interrupt("busy-thread").unwrap());
    assert!(!client.interrupt("idle-thread").unwrap());
    worker.join().unwrap();
}

#[test]
fn rpc_errors_and_eof_are_preserved() {
    let (mut client, mut server) = pair();
    let worker = thread::spawn(move || {
        let mut reader = BufReader::new(server.try_clone().unwrap());
        receive(&mut reader, 1, "initialize");
        send(
            &mut server,
            json!({"id":1,"error":{"code":-32011,"message":"fixture failure"}}),
        );
    });
    match client.initialize("fixture-version") {
        Err(CodexError::Rpc { code, message }) => {
            assert_eq!(code, -32011);
            assert_eq!(message, "fixture failure");
        }
        result => panic!("expected RPC error, got {result:?}"),
    }
    worker.join().unwrap();
    let (mut client, server) = pair();
    let worker = thread::spawn(move || {
        let mut reader = BufReader::new(server);
        receive(&mut reader, 1, "initialize");
    });
    assert!(matches!(
        client.initialize("fixture-version"),
        Err(CodexError::Closed)
    ));
    worker.join().unwrap();
}
