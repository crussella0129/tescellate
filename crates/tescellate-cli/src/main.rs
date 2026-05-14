//! `tescellate-core` binary — the headless driver Electron spawns.
//!
//! Reads LSP-framed JSON-RPC from stdin, dispatches against a long-lived
//! `WorkbookEngine`, writes responses to stdout.

use serde::Deserialize;
use serde_json::{json, Value};
use tescellate_core::{BoundedExtent, SheetExtent, SheetId};
use tescellate_formula::WorkbookEngine;
use tescellate_ipc::{serve, Request, Response};
use tescellate_tess::LatticeKind;

fn main() -> std::io::Result<()> {
    let stdin = std::io::stdin().lock();
    let stdout = std::io::stdout().lock();
    let mut engine = WorkbookEngine::new();
    // Start with an empty workbook — no sheet yet. The wizard creates the
    // first sheet via `workbook.create`, which is how the renderer knows
    // what tessellation / extent to use.
    engine.new_workbook();

    serve(stdin, stdout, |req| dispatch(&mut engine, req))
}

fn dispatch(engine: &mut WorkbookEngine, req: Request) -> Response {
    let id = req.id.clone().unwrap_or(Value::Null);
    match req.method.as_str() {
        "ping" => Response::ok(id, json!({"ok": true, "echo": req.params})),

        "workbook.new" => {
            engine.new_workbook();
            Response::ok(id, json!({"ok": true}))
        }

        "workbook.create" => match decode::<WorkbookCreateParams>(&req.params) {
            Ok(p) => match parse_lattice(&p.lattice) {
                Err(e) => Response::err(id, -32602, e),
                Ok(lattice) => {
                    let extent = parse_extent(&p.extent, &lattice);
                    match extent {
                        Err(e) => Response::err(id, -32602, e),
                        Ok(extent) => {
                            engine.new_workbook();
                            let name = p.name.unwrap_or_else(|| "Sheet1".into());
                            let sid = engine.add_sheet_with_extent(&name, lattice, extent.clone());
                            Response::ok(
                                id,
                                json!({
                                    "ok": true,
                                    "sheet": sid.0,
                                    "name": name,
                                    "lattice": lattice,
                                    "extent": extent,
                                }),
                            )
                        }
                    }
                }
            },
            Err(e) => Response::err(id, -32602, e),
        },

        "workbook.info" => {
            let sheets: Vec<_> = engine
                .workbook
                .sheet_order
                .iter()
                .filter_map(|sid| {
                    engine.workbook.sheets.get(sid).map(|s| {
                        json!({
                            "id": s.id.0,
                            "name": s.name,
                            "lattice": s.lattice,
                            "extent": s.extent,
                        })
                    })
                })
                .collect();
            Response::ok(id, json!({"sheets": sheets}))
        }

        "formula.eval" => match decode::<FormulaEvalParams>(&req.params) {
            Ok(p) => match engine.eval_literal(&p.source) {
                Ok(v) => Response::ok(id, serde_json::to_value(v).unwrap()),
                Err(e) => Response::err(id, -32000, e.to_string()),
            },
            Err(e) => Response::err(id, -32602, e),
        },

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

        "workbook.save" => match decode::<PathParams>(&req.params) {
            Ok(p) => match engine.save(std::path::Path::new(&p.path)) {
                Ok(()) => Response::ok(id, json!({"ok": true, "path": p.path})),
                Err(e) => Response::err(id, -32000, e.to_string()),
            },
            Err(e) => Response::err(id, -32602, e),
        },

        "workbook.open" => match decode::<PathParams>(&req.params) {
            Ok(p) => match engine.open(std::path::Path::new(&p.path)) {
                Ok(()) => {
                    let sheets: Vec<_> = engine
                        .workbook
                        .sheet_order
                        .iter()
                        .filter_map(|sid| {
                            engine.workbook.sheets.get(sid).map(|s| {
                                json!({
                                    "id": s.id.0,
                                    "name": s.name,
                                    "lattice": s.lattice,
                                    "extent": s.extent,
                                })
                            })
                        })
                        .collect();
                    Response::ok(id, json!({"ok": true, "path": p.path, "sheets": sheets}))
                }
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

fn parse_lattice(s: &str) -> Result<LatticeKind, String> {
    match s {
        "square" => Ok(LatticeKind::Square),
        "hex_pointy" => Ok(LatticeKind::HexPointy),
        "hex_flat" => Ok(LatticeKind::HexFlat),
        "triangle" => Ok(LatticeKind::Triangle),
        "parallelogram" => Ok(LatticeKind::Parallelogram),
        other => Err(format!("unknown lattice: {other}")),
    }
}

/// Decode a wizard-supplied extent payload into a `SheetExtent`. Validates
/// that the extent's lattice variant matches the sheet's lattice.
fn parse_extent(payload: &ExtentParams, lattice: &LatticeKind) -> Result<SheetExtent, String> {
    if payload.kind == "unbounded" {
        return Ok(SheetExtent::Unbounded);
    }
    if payload.kind != "bounded" {
        return Err(format!("unknown extent kind: {}", payload.kind));
    }
    let cols = payload.cols.unwrap_or(0);
    let rows = payload.rows.unwrap_or(0);
    if cols == 0 || rows == 0 {
        return Err("bounded extent needs cols and rows > 0".into());
    }
    match lattice {
        LatticeKind::Square => Ok(SheetExtent::Bounded(BoundedExtent::Square { cols, rows })),
        other => Err(format!(
            "bounded extent for {other:?} is a later-phase feature"
        )),
    }
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

#[derive(Deserialize)]
struct PathParams {
    path: String,
}

#[derive(Deserialize)]
struct WorkbookCreateParams {
    #[serde(default)]
    name: Option<String>,
    lattice: String,
    extent: ExtentParams,
}

#[derive(Deserialize)]
struct ExtentParams {
    /// "unbounded" | "bounded"
    kind: String,
    #[serde(default)]
    cols: Option<u32>,
    #[serde(default)]
    rows: Option<u32>,
}

#[derive(Deserialize)]
struct FormulaEvalParams {
    source: String,
}
