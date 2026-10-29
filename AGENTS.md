# Agent contract (read first)

Project: Alloy, a Matrix client (Rust core + Tauri 2 + Svelte 5). Scope/decisions: `docs/PLAN.md`. Workflow and traceability: `docs/TRACKING.md`.

1. Work is defined by a GitHub issue: `gh issue view <N>`. Stay inside its **Owned paths**.
2. Do NOT call `scripts/board.sh` or `gh project`/`gh issue` write commands (GitHub GraphQL rate limit; the orchestrator is the single board writer). Read the issue with `gh issue view <N>` only; report findings in your final message.
3. Commit the change set you own as soon as it is done: `git add` the **explicit paths you changed** and `git commit` with `type(scope): summary` plus `Refs #<N>`, then `git push origin main`. Stage explicit paths, not the whole tree — the worktree is shared with sibling agents and their uncommitted work is not yours. NEVER run `git reset`, `git stash`, `git checkout`/`git restore`, `git rebase` or `git rm`, never force-push or rewrite published history, and never delete or revert files you did not create; unexpected files belong to the user or a sibling agent. One commit per issue. If `index.lock` exists, wait and retry — never remove it.
4. Use `CARGO_TARGET_DIR=/tmp/alloy-target` — one shared target dir for the whole team, so cargo's own lock serialises builds instead of every agent rebuilding the same crates. Kill any command running > 5 min and investigate. **Clean up before your final report**: `rm -rf` temp files and anything you started (docker containers/images, dev servers); never leave build output behind (`scripts/clean-artifacts.sh` removes leftovers).
5. Run fmt/clippy `-D warnings`/tests only for your own crate/package (workspace gates are the orchestrator's). No placeholder code or fake tests; if something can't be done, say so with the reason.
6. Final report: files changed, exact commands and real output, the commit SHA(s), anything incomplete. Each commit carries `Refs #<N>`; a change set that spans your area and someone else's is committed by its author with the other's lines named in the commit body.
7. No iOS work (decision D10). Targets: macOS/Windows/Linux + Android.
8. UI work must follow `docs/decisions/0001-ui-visual-language.md` (no emoji/decorative icons, toggle theme switch, per-user name colors, Telegram-style bubbles).
