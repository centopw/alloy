# Agent contract (read first)

Project: Alloy, a Matrix client (Rust core + Tauri 2 + Svelte 5). Scope/decisions: `docs/PLAN.md`. Workflow and traceability: `docs/TRACKING.md`.

1. Work is defined by a GitHub issue: `gh issue view <N>`. Stay inside its **Owned paths**.
2. `scripts/board.sh claim <N>` when you start (status In progress); post findings as issue comments.
3. NEVER run git write commands (add, commit, reset, stash, checkout, rm, rebase, push) and NEVER delete or revert files you did not create; unexpected files belong to the user or a sibling agent.
4. Use `CARGO_TARGET_DIR=/tmp/alloy-target-<name>`; kill any command running > 5 min and investigate.
5. Run fmt/clippy `-D warnings`/tests only for your own crate/package (workspace gates are the orchestrator's). No placeholder code or fake tests; if something can't be done, say so with the reason.
6. Final report: files changed, exact commands and real output, anything incomplete. The orchestrator commits with `Refs #<N>`.
7. No iOS work (decision D10). Targets: macOS/Windows/Linux + Android.
