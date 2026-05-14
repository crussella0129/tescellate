//! End-to-end framing test: a length-prefixed JSON-RPC `ping` round-trips
//! through the server loop. This is the Phase 0 smoke test — if this
//! passes the renderer ↔ main ↔ core wire is sound.

use std::io::{Cursor, Read};

#[test]
fn ping_round_trip() {
    let body = br#"{"jsonrpc":"2.0","id":42,"method":"ping","params":{"hello":"world"}}"#;
    let mut input = Vec::new();
    input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    input.extend_from_slice(body);

    let reader = Cursor::new(input);
    let mut output: Vec<u8> = Vec::new();
    tescellate_ipc::serve(reader, &mut output, |req| {
        let id = req.id.unwrap_or(serde_json::Value::Null);
        tescellate_ipc::Response::ok(id, serde_json::json!({"ok": true, "echo": req.params}))
    })
    .expect("serve loop");

    // Parse the response frame back out.
    let mut cursor = Cursor::new(&output);
    let mut header = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        if cursor.read(&mut byte).unwrap() == 0 {
            panic!("EOF reading header");
        }
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let header_str = std::str::from_utf8(&header).unwrap();
    let len: usize = header_str
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length:"))
        .expect("Content-Length")
        .trim()
        .parse()
        .unwrap();
    let mut body = vec![0u8; len];
    cursor.read_exact(&mut body).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["jsonrpc"], "2.0");
    assert_eq!(parsed["id"], 42);
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["echo"]["hello"], "world");
}
