# Sprint 0 End-to-End Tests

**Status:** not yet possible — unlocked by sprint 1.

The plan's three E2E targets (browser save/open round-trip, browser
autosave-survives-refresh, native save/open round-trip) all require the
Save/Open dialog flow that was deferred from sprint 0 to sprint 1. None
can run against the v144 build.

The engine + serialization infrastructure that sprint 0 ships is verified
end-to-end at the byte-API level (see `unit-tests.md` T-003 / T-004 /
T-005 / T-006) — that's the largest piece of E2E behavior that can be
exercised without a user-facing dialog.
