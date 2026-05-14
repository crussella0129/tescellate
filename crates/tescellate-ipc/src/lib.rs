//! JSON-RPC 2.0 IPC server for the Tescellate core. See PLAN.md §7.
//!
//! Phase 0 implements the minimum to prove the loop:
//! - read length-prefixed JSON-RPC frames from stdin
//! - dispatch a single `ping` method
//! - write the response to stdout
//!
//! Phase 1 expands this to the methods listed in PLAN.md §7.1.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};

#[derive(Debug, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

/// Minimal blocking server. Frames are `Content-Length: N\r\n\r\n<json>`
/// LSP-style — easy to drive from Node.js on the Electron side.
pub fn serve<R: Read, W: Write>(reader: R, mut writer: W) -> std::io::Result<()> {
    let mut buf = BufReader::new(reader);
    loop {
        // Parse headers
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let n = buf.read_line(&mut line)?;
            if n == 0 {
                return Ok(()); // EOF
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }
            if let Some(v) = trimmed.strip_prefix("Content-Length:") {
                content_length = Some(v.trim().parse().unwrap_or(0));
            }
        }
        let len = match content_length {
            Some(n) => n,
            None => continue,
        };
        let mut body = vec![0u8; len];
        buf.read_exact(&mut body)?;
        let req: Request = match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(e) => {
                write_response(
                    &mut writer,
                    &Response {
                        jsonrpc: "2.0",
                        id: serde_json::Value::Null,
                        result: None,
                        error: Some(RpcError {
                            code: -32700,
                            message: format!("parse error: {e}"),
                        }),
                    },
                )?;
                continue;
            }
        };
        let resp = handle(&req);
        write_response(&mut writer, &resp)?;
    }
}

fn handle(req: &Request) -> Response {
    let id = req.id.clone().unwrap_or(serde_json::Value::Null);
    match req.method.as_str() {
        "ping" => Response {
            jsonrpc: "2.0",
            id,
            result: Some(serde_json::json!({"ok": true, "echo": req.params})),
            error: None,
        },
        _ => Response {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code: -32601,
                message: format!("method not found: {}", req.method),
            }),
        },
    }
}

fn write_response<W: Write>(w: &mut W, resp: &Response) -> std::io::Result<()> {
    let body = serde_json::to_vec(resp)?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()?;
    Ok(())
}
