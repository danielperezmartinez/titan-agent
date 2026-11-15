mod support;

use base64::Engine;
use std::io::Write;
use std::time::{Duration, Instant};
use support::{TIMEOUT, frame, read_client_frame, read_upgrade, server, upgrade};
use titan_agentd::ws::{WsClient, WsError};

#[test]
fn upgrade_has_required_headers_and_preserves_the_first_frame() {
    let (_dir, path, handle) = server(|mut stream| {
        let request = read_upgrade(&mut stream);
        assert!(request.starts_with("GET / HTTP/1.1\r\n"));
        let headers: Vec<_> = request
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once(':'))
            .collect();
        for (name, expected) in [
            ("Host", "localhost"),
            ("Upgrade", "websocket"),
            ("Connection", "Upgrade"),
            ("Sec-WebSocket-Version", "13"),
        ] {
            assert!(
                headers
                    .iter()
                    .any(|(key, value)| key.eq_ignore_ascii_case(name)
                        && value.trim().eq_ignore_ascii_case(expected))
            );
        }
        let key = headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("Sec-WebSocket-Key"))
            .unwrap()
            .1
            .trim();
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(key)
                .unwrap()
                .len(),
            16
        );
        let mut reply = b"HTTP/1.1 101 Switching Protocols\r\n\r\n".to_vec();
        reply.extend(frame(true, 1, b"first"));
        stream.write_all(&reply).unwrap();
    });
    let mut client = WsClient::connect(&path, TIMEOUT).unwrap();
    assert_eq!(client.recv_text(TIMEOUT).unwrap(), "first");
    handle.join().unwrap();
}

#[test]
fn text_round_trips_all_length_encodings_in_single_masked_frames() {
    let (_dir, path, handle) = server(|mut stream| {
        upgrade(&mut stream);
        for length in [0, 5, 125, 126, 65535, 70000] {
            let (fin, opcode, payload) = read_client_frame(&mut stream);
            assert!(fin);
            assert_eq!(opcode, 1);
            assert_eq!(payload, vec![b'x'; length]);
            stream.write_all(&frame(true, 1, &payload)).unwrap();
        }
    });
    let mut client = WsClient::connect(&path, TIMEOUT).unwrap();
    for length in [0, 5, 125, 126, 65535, 70000] {
        let text = "x".repeat(length);
        client.send_text(&text).unwrap();
        assert_eq!(client.recv_text(TIMEOUT).unwrap(), text);
    }
    handle.join().unwrap();
}

#[test]
fn fragments_reassemble_across_ping_and_ignore_pong_and_binary() {
    let (_dir, path, handle) = server(|mut stream| {
        upgrade(&mut stream);
        // The UTF-8 character is split across continuation frames.
        for bytes in [
            frame(true, 2, b"ignored"),
            frame(true, 10, b"ignored"),
            frame(false, 1, &[b'a', 0xc3]),
            frame(true, 9, b"probe"),
            frame(true, 0, &[0xa9, b'z']),
        ] {
            stream.write_all(&bytes).unwrap();
        }
        let (fin, opcode, payload) = read_client_frame(&mut stream);
        assert!(fin);
        assert_eq!(opcode, 10);
        assert_eq!(payload, b"probe");
    });
    let mut client = WsClient::connect(&path, TIMEOUT).unwrap();
    assert_eq!(client.recv_text(TIMEOUT).unwrap(), "aéz");
    handle.join().unwrap();
}

#[test]
fn close_frame_returns_closed() {
    let (_dir, path, handle) = server(|mut stream| {
        upgrade(&mut stream);
        stream
            .write_all(&frame(true, 8, &1000_u16.to_be_bytes()))
            .unwrap();
    });
    let mut client = WsClient::connect(&path, TIMEOUT).unwrap();
    assert!(matches!(client.recv_text(TIMEOUT), Err(WsError::Closed)));
    handle.join().unwrap();
}

#[test]
fn non_upgrade_status_is_a_handshake_error() {
    let (_dir, path, handle) = server(|mut stream| {
        read_upgrade(&mut stream);
        stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });
    assert!(matches!(
        WsClient::connect(&path, TIMEOUT),
        Err(WsError::Handshake(_))
    ));
    handle.join().unwrap();
}

#[test]
fn receive_deadline_returns_an_io_timeout() {
    let (release, wait) = std::sync::mpsc::channel();
    let (_dir, path, handle) = server(move |mut stream| {
        upgrade(&mut stream);
        let _ = wait.recv_timeout(TIMEOUT);
    });
    let mut client = WsClient::connect(&path, TIMEOUT).unwrap();
    let started = Instant::now();
    let result = client.recv_text(Duration::from_millis(80));
    let elapsed = started.elapsed();
    release.send(()).unwrap();
    handle.join().unwrap();
    assert!(
        matches!(result, Err(WsError::Io(error)) if matches!(error.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock))
    );
    assert!(elapsed < Duration::from_secs(1));
}

#[test]
fn oversized_text_message_is_a_protocol_error() {
    let (_dir, path, handle) = server(|mut stream| {
        upgrade(&mut stream);
        let bytes = frame(true, 1, &vec![b'x'; 16 * 1024 * 1024 + 1]);
        // The client may reject the declared length before consuming the body.
        let _ = stream.write_all(&bytes);
    });
    let mut client = WsClient::connect(&path, TIMEOUT).unwrap();
    assert!(matches!(
        client.recv_text(TIMEOUT),
        Err(WsError::Protocol(_))
    ));
    drop(client);
    handle.join().unwrap();
}
