//! Dependency graph for the workbook. See PLAN.md §5.
//!
//! Two maps: `forward[c]` is what `c` reads, `reverse[c]` is who reads `c`.
//! `set_deps` rejects edge additions that introduce a cycle (so recompute
//! never has to defend against an SCC on the hot path).

use crate::reference::CellRef;
use hashbrown::HashMap;
use smallvec::SmallVec;
use thiserror::Error;

#[derive(Debug, Clone, Default)]
pub struct Dag {
    forward: HashMap<CellRef, SmallVec<[CellRef; 4]>>,
    reverse: HashMap<CellRef, SmallVec<[CellRef; 8]>>,
}

#[derive(Debug, Error)]
pub enum DagError {
    #[error("cycle through {cell:?}")]
    Cycle { cell: CellRef },
}

impl Dag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn deps_of(&self, cell: &CellRef) -> &[CellRef] {
        self.forward.get(cell).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn dependents_of(&self, cell: &CellRef) -> &[CellRef] {
        self.reverse.get(cell).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Set the outgoing dependency edges of `cell` to `new_deps`, replacing
    /// any previous edges. Rejects with `DagError::Cycle` if the resulting
    /// graph would contain a cycle through `cell`.
    pub fn set_deps(&mut self, cell: &CellRef, new_deps: Vec<CellRef>) -> Result<(), DagError> {
        // Detect cycles before mutating: if any of new_deps transitively
        // depends on `cell`, the edge cell→dep would close a cycle.
        for dep in &new_deps {
            if dep == cell || self.transitively_depends_on(dep, cell) {
                return Err(DagError::Cycle { cell: cell.clone() });
            }
        }

        // Remove old reverse edges from previous deps.
        if let Some(old) = self.forward.get(cell) {
            for old_dep in old.clone() {
                if let Some(rev) = self.reverse.get_mut(&old_dep) {
                    rev.retain(|x| x != cell);
                }
            }
        }

        // Install new edges.
        for dep in &new_deps {
            self.reverse
                .entry(dep.clone())
                .or_default()
                .push(cell.clone());
        }
        if new_deps.is_empty() {
            self.forward.remove(cell);
        } else {
            self.forward.insert(cell.clone(), new_deps.into());
        }
        Ok(())
    }

    /// Returns true if `from` transitively depends on `target` via forward edges.
    fn transitively_depends_on(&self, from: &CellRef, target: &CellRef) -> bool {
        let mut stack: Vec<CellRef> = vec![from.clone()];
        let mut seen: std::collections::HashSet<CellRef> = std::collections::HashSet::new();
        while let Some(c) = stack.pop() {
            if &c == target {
                return true;
            }
            if !seen.insert(c.clone()) {
                continue;
            }
            if let Some(deps) = self.forward.get(&c) {
                for d in deps {
                    stack.push(d.clone());
                }
            }
        }
        false
    }

    /// BFS from `root` over the reverse edges — every cell that needs to
    /// be re-evaluated when `root` changes. Includes `root` itself as the
    /// first element.
    pub fn dirty_closure(&self, root: &CellRef) -> Vec<CellRef> {
        let mut out = vec![root.clone()];
        let mut i = 0;
        let mut seen: std::collections::HashSet<CellRef> = std::collections::HashSet::new();
        seen.insert(root.clone());
        while i < out.len() {
            let cur = out[i].clone();
            i += 1;
            if let Some(deps) = self.reverse.get(&cur) {
                for d in deps {
                    if seen.insert(d.clone()) {
                        out.push(d.clone());
                    }
                }
            }
        }
        out
    }

    /// Topological order over the subset, using Kahn's algorithm restricted
    /// to nodes within `subset`. Edges that point outside the subset are
    /// ignored for ordering purposes (those external nodes are assumed to
    /// already be up to date).
    pub fn topo_order(&self, subset: Vec<CellRef>) -> Vec<CellRef> {
        use std::collections::HashSet as StdHashSet;
        let set: StdHashSet<CellRef> = subset.iter().cloned().collect();
        let mut indeg: HashMap<CellRef, usize> = HashMap::new();
        for c in &subset {
            let n = self
                .forward
                .get(c)
                .map(|v| v.iter().filter(|d| set.contains(*d)).count())
                .unwrap_or(0);
            indeg.insert(c.clone(), n);
        }
        let mut queue: Vec<CellRef> = indeg
            .iter()
            .filter_map(|(k, v)| if *v == 0 { Some(k.clone()) } else { None })
            .collect();
        let mut out = Vec::with_capacity(subset.len());
        while let Some(c) = queue.pop() {
            out.push(c.clone());
            if let Some(deps) = self.reverse.get(&c) {
                for d in deps {
                    if let Some(n) = indeg.get_mut(d) {
                        *n -= 1;
                        if *n == 0 {
                            queue.push(d.clone());
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SheetId;

    fn cref(s: u32, addr: &str) -> CellRef {
        CellRef::new(SheetId(s), addr)
    }

    #[test]
    fn linear_chain_topo_order_is_root_first() {
        let mut dag = Dag::new();
        let a = cref(1, "A1");
        let b = cref(1, "B1");
        let c = cref(1, "C1");
        dag.set_deps(&b, vec![a.clone()]).unwrap();
        dag.set_deps(&c, vec![b.clone()]).unwrap();
        let dirty = dag.dirty_closure(&a);
        assert_eq!(dirty.len(), 3);
        let ordered = dag.topo_order(dirty);
        let pos = |x: &CellRef| ordered.iter().position(|y| y == x).unwrap();
        assert!(pos(&a) < pos(&b));
        assert!(pos(&b) < pos(&c));
    }

    #[test]
    fn diamond_dependency() {
        let mut dag = Dag::new();
        let a = cref(1, "A1");
        let b = cref(1, "B1");
        let c = cref(1, "C1");
        let d = cref(1, "D1");
        dag.set_deps(&b, vec![a.clone()]).unwrap();
        dag.set_deps(&c, vec![a.clone()]).unwrap();
        dag.set_deps(&d, vec![b.clone(), c.clone()]).unwrap();
        let dirty = dag.dirty_closure(&a);
        assert_eq!(dirty.len(), 4);
        let ordered = dag.topo_order(dirty);
        let pos = |x: &CellRef| ordered.iter().position(|y| y == x).unwrap();
        assert!(pos(&a) < pos(&b));
        assert!(pos(&a) < pos(&c));
        assert!(pos(&b) < pos(&d));
        assert!(pos(&c) < pos(&d));
    }

    #[test]
    fn cycle_is_rejected() {
        let mut dag = Dag::new();
        let a = cref(1, "A1");
        let b = cref(1, "B1");
        dag.set_deps(&b, vec![a.clone()]).unwrap();
        assert!(matches!(
            dag.set_deps(&a, vec![b.clone()]),
            Err(DagError::Cycle { .. })
        ));
    }

    #[test]
    fn self_reference_is_rejected() {
        let mut dag = Dag::new();
        let a = cref(1, "A1");
        assert!(matches!(
            dag.set_deps(&a, vec![a.clone()]),
            Err(DagError::Cycle { .. })
        ));
    }

    #[test]
    fn replacing_deps_drops_old_reverse_edges() {
        let mut dag = Dag::new();
        let a = cref(1, "A1");
        let b = cref(1, "B1");
        let c = cref(1, "C1");
        dag.set_deps(&c, vec![a.clone()]).unwrap();
        assert_eq!(dag.dependents_of(&a), std::slice::from_ref(&c));
        dag.set_deps(&c, vec![b.clone()]).unwrap();
        assert!(dag.dependents_of(&a).is_empty());
        assert_eq!(dag.dependents_of(&b), std::slice::from_ref(&c));
    }
}
