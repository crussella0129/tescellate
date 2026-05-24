# Agent Tasks (Persistent Backlog)

## Sprint 19 — Carbide rename + stale-app cleanup (v161)

- **T-001** — Delete `apps/desktop` (Electron renderer) + remove CI `frontend` job.
- **T-002** — Delete `crates/tescellate-ipc`; collapse `tescellate-cli` to a `--help` stub; verify formula/store tests cover the e2e surface before deleting the e2e test; enumerate runtime cache-path strings.
- **T-003** — `git mv` 5 crates + `apps/tescellate-ui`; update `Cargo.toml` workspace `members =` AND `[workspace.dependencies]` paths AND `apps/carbide-ui/Cargo.toml` `[dependencies]` paths.
- **T-004** — Perl sweep over `git ls-files` (excl. `agent-tasks/`, `.gitignore`, 3 repo-URL sites); restore ADR titles in `decisions.md`; regenerate `Cargo.lock` + `apps/carbide-ui/Cargo.lock`; verify with tightened greps.
- **T-005** — `.tscl` → `.crbd`: file-dialog filters (Open accepts both `crbd`+`tscl`; Save defaults to `.crbd`); doc-string `.tscl` → `.crbd` sweep; `.gitignore` hand-edit to add `*.crbd` and `!examples/*.crbd` while preserving `*.tscl` rules.
- **T-006** — `old_tscl_bytes_still_load_after_rename` unit test in `carbide-store`; commit `examples/sprint19-back-compat.tscl` for the visual checkpoint.
- **T-007** — ADR-015 in `decisions.md` with cross-refs to ADR-001/012/013/014; document the post-merge user-driven repo-rename + URL-flip follow-up.
