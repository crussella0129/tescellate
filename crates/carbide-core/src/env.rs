//! Lexical environment for Carbide formulas. See PLAN.md §6.2.4.
//!
//! `Env` is a persistent (Arc-shared) chain of binding scopes used by
//! `LET`, `LAMBDA`, and `LETREC` to model lexical scope. Lambda values
//! (`CellValue::Function`) capture an `Arc<Env>` at definition time, so
//! they see the same bindings their definition site saw — even after
//! `LETREC` patches placeholder slots, because the env chain is shared
//! through the `Arc` and the bindings hash map is mutable through `RwLock`.
//!
//! Lives in `carbide-core` (not `carbide-formula`) because `CellValue`
//! lives here and any lambda value must carry an `Arc<Env>`.

use crate::CellValue;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug)]
pub struct Env {
    pub bindings: RwLock<HashMap<String, CellValue>>,
    pub parent: Option<Arc<Env>>,
}

impl Env {
    /// Empty top-level scope.
    pub fn empty_arc() -> Arc<Self> {
        Arc::new(Self {
            bindings: RwLock::new(HashMap::new()),
            parent: None,
        })
    }

    /// New scope that chains to `parent` — names not found locally fall
    /// through to the parent (and its parent, and so on).
    pub fn child_of(parent: Arc<Env>) -> Arc<Self> {
        Arc::new(Self {
            bindings: RwLock::new(HashMap::new()),
            parent: Some(parent),
        })
    }

    /// Walk the parent chain looking for `name`. First hit wins, so child
    /// bindings shadow parent bindings (standard lexical-scope semantics).
    pub fn lookup(&self, name: &str) -> Option<CellValue> {
        if let Some(v) = self.bindings.read().ok().and_then(|b| b.get(name).cloned()) {
            return Some(v);
        }
        self.parent.as_ref().and_then(|p| p.lookup(name))
    }

    /// Insert or replace a binding in this scope only. Used by LET (one
    /// pass) and LETREC (placeholder pass, then patch pass).
    pub fn insert(&self, name: impl Into<String>, value: CellValue) {
        if let Ok(mut b) = self.bindings.write() {
            b.insert(name.into(), value);
        }
    }
}
