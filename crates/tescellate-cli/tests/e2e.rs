//! End-to-end test of the `tescellate-core` CLI — v15.
//!
//! Every other test in this workspace drives the engine *in process*, by
//! calling library APIs directly. This one drives the **real binary**: it
//! spawns `tescellate-core`, speaks its LSP-framed JSON-RPC stdio protocol
//! over a pipe exactly as the Electron front-end does, and asserts on the
//! responses. It is the only test that exercises the integrated stack —
//! IPC framing → JSON-RPC dispatch → parse → DAG → eval → serialization —
//! through the actual process boundary.
//!
//! Protocol (see `tescellate-ipc`): each message is
//! `Content-Length: <n>\r\n\r\n<json>`; the body is JSON-RPC 2.0. The
//! server starts with an empty workbook and exits when stdin reaches EOF.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

/// A live `tescellate-core` subprocess plus a JSON-RPC client speaking its
/// LSP-framed stdio protocol.
struct CliSession {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl CliSession {
    /// Spawn the binary with piped stdin/stdout. `CARGO_BIN_EXE_*` is set
    /// by Cargo for this package's integration tests, so the binary is
    /// always freshly built.
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tescellate-core"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn the tescellate-core binary");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        CliSession {
            child,
            stdin: Some(stdin),
            stdout,
            next_id: 1,
        }
    }

    /// Send a JSON-RPC request and return the full response object.
    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let body = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .expect("serialize request");

        let stdin = self.stdin.as_mut().expect("session still open");
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write framing header");
        stdin.write_all(&body).expect("write request body");
        stdin.flush().expect("flush request");

        self.read_response(id)
    }

    /// Send a request and return its `result`, asserting no JSON-RPC error.
    fn result(&mut self, method: &str, params: Value) -> Value {
        let resp = self.call(method, params);
        assert!(
            resp.get("error").is_none(),
            "`{method}` returned a JSON-RPC error: {resp}",
        );
        resp.get("result").cloned().unwrap_or(Value::Null)
    }

    fn read_response(&mut self, expect_id: i64) -> Value {
        // LSP framing: `Content-Length: N` headers, a blank line, then
        // exactly N body bytes.
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).expect("read header line");
            assert!(n != 0, "tescellate-core closed the pipe before responding");
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                content_length = Some(rest.trim().parse().expect("Content-Length is an integer"));
            }
        }
        let len = content_length.expect("response framing carried a Content-Length");
        let mut body = vec![0u8; len];
        self.stdout
            .read_exact(&mut body)
            .expect("read response body");

        let resp: Value = serde_json::from_slice(&body).expect("response body is JSON");
        assert_eq!(
            resp["id"].as_i64(),
            Some(expect_id),
            "response id did not match the request that was sent",
        );
        resp
    }

    /// Close stdin (EOF → the server exits its read loop) and confirm a
    /// clean exit.
    fn shutdown(&mut self) {
        self.stdin = None;
        let status = self.child.wait().expect("wait for tescellate-core to exit");
        assert!(
            status.success(),
            "tescellate-core exited unsuccessfully: {status}",
        );
    }
}

impl Drop for CliSession {
    fn drop(&mut self) {
        // Safety net: if a test panicked before `shutdown`, don't leak the
        // process. `kill`/`wait` on an already-exited child error harmlessly.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `workbook.create` a bounded square sheet and return its sheet id.
fn create_square_sheet(cli: &mut CliSession) -> u64 {
    let created = cli.result(
        "workbook.create",
        json!({
            "name": "Sheet1",
            "lattice": "square",
            "extent": {"kind": "bounded", "cols": 16, "rows": 16},
        }),
    );
    created["sheet"]
        .as_u64()
        .expect("workbook.create result carries a numeric sheet id")
}

#[test]
fn cli_drives_a_workbook_end_to_end() {
    let mut cli = CliSession::start();

    // The simplest round-trip — proves framing and dispatch work at all.
    assert_eq!(
        cli.result("ping", json!({"hello": "world"}))["ok"],
        json!(true),
    );

    let sheet = create_square_sheet(&mut cli);

    // A literal, and a formula that depends on it.
    cli.result(
        "cell.set",
        json!({"sheet": sheet, "address": "A1", "source": "=6"}),
    );
    cli.result(
        "cell.set",
        json!({"sheet": sheet, "address": "A2", "source": "=A1 * 7"}),
    );
    assert_eq!(
        cli.result("cell.get", json!({"sheet": sheet, "address": "A2"}))["value"],
        json!({"kind": "number", "value": 42.0}),
        "A2 should evaluate A1*7 = 42",
    );

    // Editing A1 must propagate through the DAG to A2.
    cli.result(
        "cell.set",
        json!({"sheet": sheet, "address": "A1", "source": "=10"}),
    );
    assert_eq!(
        cli.result("cell.get", json!({"sheet": sheet, "address": "A2"}))["value"],
        json!({"kind": "number", "value": 70.0}),
        "A2 should recompute to A1*7 = 70 after A1 is edited",
    );

    cli.shutdown();
}

#[test]
fn cli_evaluates_standalone_formulas() {
    let mut cli = CliSession::start();
    for (src, expect) in [
        ("=2 + 3 * 4", 14.0),
        ("=(2 + 3) * 4", 20.0),
        ("=2 ^ 10", 1024.0),
        ("=SUM(1, 2, 3, 4)", 10.0),
    ] {
        assert_eq!(
            cli.result("formula.eval", json!({"source": src})),
            json!({"kind": "number", "value": expect}),
            "formula.eval of {src}",
        );
    }
    cli.shutdown();
}

#[test]
fn cli_surfaces_errors_over_rpc() {
    let mut cli = CliSession::start();
    let sheet = create_square_sheet(&mut cli);

    // A *formula* error is a structured cell value, not an RPC-level error.
    let changed = cli.result(
        "cell.set",
        json!({"sheet": sheet, "address": "A1", "source": "=1 / 0"}),
    );
    let value = &changed
        .as_array()
        .expect("cell.set returns the array of changed cells")[0]["value"];
    assert_eq!(
        value,
        &json!({"kind": "error", "value": {"code": "div_zero"}}),
        "a divide-by-zero should surface as a DivZero cell error, got {value}",
    );

    // An unknown *method* is a JSON-RPC protocol error (-32601).
    let resp = cli.call("no.such.method", json!({}));
    assert_eq!(
        resp["error"]["code"],
        json!(-32601),
        "an unknown method should yield JSON-RPC error -32601, got {resp}",
    );

    cli.shutdown();
}

#[test]
fn cli_snapshots_a_populated_range() {
    let mut cli = CliSession::start();
    let sheet = create_square_sheet(&mut cli);

    for (addr, src) in [("A1", "=1"), ("A2", "=2"), ("B1", "=3")] {
        cli.result(
            "cell.set",
            json!({"sheet": sheet, "address": addr, "source": src}),
        );
    }
    let snap = cli.result(
        "range.snapshot",
        json!({"sheet": sheet, "start": "A1", "end": "B2"}),
    );
    let cells = snap.as_array().expect("range.snapshot returns an array");
    assert_eq!(
        cells.len(),
        3,
        "range A1:B2 should report exactly the 3 populated cells",
    );

    cli.shutdown();
}
