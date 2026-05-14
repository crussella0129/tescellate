//! JSON-RPC 2.0 IPC server for the Tescellate core. See PLAN.md §7.
//!
//! Framing only — the dispatcher lives in the caller (`tescellate-cli`)
//! so this crate stays free of workbook-state dependencies and remains
//! useful for tests, alternate transports, and headless tools.

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

#[derive(Debug, Serialize, Clone)]
pub struct Response {
    pub jsonrpc: &'static str,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl Response {
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: serde_json::Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

/// Blocking server loop. Frames are LSP-style `Content-Length` headers
/// followed by JSON-RPC bodies. The caller supplies the per-request
/// handler so this crate stays state-free.
pub fn serve<R, W, F>(reader: R, mut writer: W, mut handler: F) -> std::io::Result<()>
where
    R: Read,
    W: Write,
    F: FnMut(Request) -> Response,
{
    let mut buf = BufReader::new(reader);
    loop {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let n = buf.read_line(&mut line)?;
            if n == 0 {
                return Ok(());
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
                    &Response::err(serde_json::Value::Null, -32700, format!("parse error: {e}")),
                )?;
                continue;
            }
        };
        let resp = handler(req);
        write_response(&mut writer, &resp)?;
    }
}

pub fn write_response<W: Write>(w: &mut W, resp: &Response) -> std::io::Result<()> {
    let body = serde_json::to_vec(resp)?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()?;
    Ok(())
}
