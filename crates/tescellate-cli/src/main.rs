//! `tescellate-core` binary — the headless driver Electron spawns.
//!
//! Reads LSP-framed JSON-RPC from stdin, dispatches against a long-lived
//! `WorkbookEngine`, writes responses to stdout.

use serde::Deserialize;
use serde_json::{json, Value};
use tescellate_core::SheetId;
use tescellate_formula::WorkbookEngine;
use tescellate_ipc::{serve, Request, Response};
use tescellate_tess::LatticeKind;

fn main() -> std::io::Result<()> {
    let stdin = std::io::stdin().lock();
    let stdout = std::io::stdout().lock();
    let mut engine = WorkbookEngine::new();
    // Phase 1 starts with a default workbook + a single square sheet so the
    // renderer has something to talk to immediately. The frontend can later
    // call `workbook.new` to reset.
    engine.new_workbook();
    engine.add_sheet("Sheet1", LatticeKind::Square);

    serve(stdin, stdout, |req| dispatch(&mut engine, req))
}

fn dispatch(engine: &mut WorkbookEngine, req: Request) -> Response {
    let id = req.id.clone().unwrap_or(Value::Null);
    match req.method.as_str() {
        "ping" => Response::ok(id, json!({"ok": true, "echo": req.params})),
        "workbook.new" => {
            engine.new_workbook();
            let sid = engine.add_sheet("Sheet1", LatticeKind::Square);
            Response::ok(id, json!({"sheet": sid.0}))
        }
        "sheet.add" => match decode::<SheetAddParams>(&req.params) {
            Ok(p) => {
                let lattice = match p.lattice.as_deref().unwrap_or("square") {
                    "square" => LatticeKind::Square,
                    other => {
                        return Response::err(id, -32602, format!("unsupported lattice: {other}"));
                    }
                };
                let sid = engine.add_sheet(p.name, lattice);
                Response::ok(id, json!({"sheet": sid.0}))
            }
            Err(e) => Response::err(id, -32602, e),
        },
        "cell.set" => match decode::<CellSetParams>(&req.params) {
            Ok(p) => match engine.set_cell(SheetId(p.sheet), &p.address, p.source.as_deref()) {
                Ok(changed) => {
                    let snapshots: Vec<_> = changed
                        .iter()
                        .filter_map(|c| engine.get_cell(c.sheet, &c.address))
                        .collect();
                    Response::ok(id, serde_json::to_value(snapshots).unwrap())
                }
                Err(e) => Response::err(id, -32000, e.to_string()),
            },
            Err(e) => Response::err(id, -32602, e),
        },
        "cell.get" => match decode::<CellGetParams>(&req.params) {
            Ok(p) => match engine.get_cell(SheetId(p.sheet), &p.address) {
                Some(s) => Response::ok(id, serde_json::to_value(s).unwrap()),
                None => Response::ok(id, Value::Null),
            },
            Err(e) => Response::err(id, -32602, e),
        },
        "range.snapshot" => match decode::<RangeSnapshotParams>(&req.params) {
            Ok(p) => match engine.snapshot_range(SheetId(p.sheet), &p.start, &p.end) {
                Ok(snap) => Response::ok(id, serde_json::to_value(snap).unwrap()),
                Err(e) => Response::err(id, -32000, e.to_string()),
            },
            Err(e) => Response::err(id, -32602, e),
        },
        other => Response::err(id, -32601, format!("method not found: {other}")),
    }
}

fn decode<T: for<'de> Deserialize<'de>>(v: &Value) -> Result<T, String> {
    serde_json::from_value(v.clone()).map_err(|e| format!("bad params: {e}"))
}

#[derive(Deserialize)]
struct SheetAddParams {
    name: String,
    #[serde(default)]
    lattice: Option<String>,
}

#[derive(Deserialize)]
struct CellSetParams {
    sheet: u32,
    address: String,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Deserialize)]
struct CellGetParams {
    sheet: u32,
    address: String,
}

#[derive(Deserialize)]
struct RangeSnapshotParams {
    sheet: u32,
    start: String,
    end: String,
}
