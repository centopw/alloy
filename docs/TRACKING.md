# Tracking & Traceability

Every feature, spike, chore and decision is traceable: **Issue -> Board -> Commit -> Verification**.

- Board: <https://github.com/users/centopw/projects/2> ("Features Release"), repo issues: <https://github.com/centopw/alloy/issues>.
- Source of truth for scope/decisions: [PLAN.md](PLAN.md). Contracts: [plan/06-ipc-contract.md](plan/06-ipc-contract.md).
- Helper: `scripts/board.sh` (needs `gh` with `repo` + `project` scopes and `jq`).

## Rules

1. **No work without an issue.** Create it first: `scripts/board.sh new --title "feat(core): ..." --milestone "v0.1 Desktop MVP" --labels "phase:P1,type:feature,area:core" --priority P1 --size M --body-file brief.md`.
2. **Issue title = commit subject style** (`type(scope): summary`). Labels: one `phase:*`, one `type:*`, one or more `area:*`. Milestone = release train (see below).
3. **Status flow** (board `Status` field): `Backlog -> Ready -> In progress -> In review -> Done`.
   - `Ready` = brief complete and all blockers done.
   - Start work: `scripts/board.sh claim <n>`.
   - Finish: `scripts/board.sh done <n> <sha>` after the commit is pushed and verification passed.
4. **Commits reference issues.** `feat|fix|perf|refactor` commits MUST contain `Refs #N` / `Closes #N` in the body (enforced by `scripts/check-commit-msg.sh`, local hooks via `just hooks`, and CI job `commits`). Other types should when applicable. One logical change per commit, linear history, push to `main` directly until the first functioning version, then PR-only.
5. **Decisions are recorded**, not implied: significant architecture/product decisions get an issue labelled `type:decision` and, if lasting, a record in [`docs/decisions/`](decisions/README.md). Changing a decision from PLAN.md section 1 means updating PLAN.md in the same commit and referencing the decision issue.
6. **Gates are recorded.** Each phase gate closes only with a comment on its issue containing the exact verification commands + results (fmt, clippy -D warnings, tests, UI check/build, deny, integration).
7. **Blockers/dependencies**: write `Blocked by #N` in the issue body; use GitHub sub-issues/`Parent issue` for epics. Do not start an issue whose blockers are open.

## Milestones (release trains)

| Milestone | Meaning |
|---|---|
| P0 Foundations | spikes, workspace, G0 |
| v0.1 Desktop MVP | P1: desktop chat MVP |
| v0.2-0.4 Android and parity | P2: Android, feature parity, tier-A customization |
| v0.5 Plugins | P3: WASM plugin host |
| v1.0 Marketplace | P4: marketplace, signed releases |
| Post-1.0 | web, calls, multi-account, widgets, iOS |

Priority: `P0` critical path/blocker, `P1` planned scope, `P2` later. Size: `XS S M L XL` (rough effort).

## Multi-agent workflow

The orchestrator dispatches **one issue per subagent** and gives each a self-contained brief (the issue body is the brief). Agents never share context, so the issue must say everything.

Issue body (use the *Work item* form) must contain:
- **Goal** and **Acceptance** (observable results).
- **Owned paths** (<= 3-5 explicit paths; disjoint from sibling issues in the same wave).
- **Contract/dependencies** (frozen interfaces consumed; `Blocked by #N`).
- **Verification** (exact commands).

Rules for agents:
- Agents never write to the board or issues (rate limits, single writer): the orchestrator updates status in batches (`scripts/board.sh`, which caches field ids and uses one GraphQL query per item).
- Read the issue (`gh issue view N`), `docs/PLAN.md`, and the relevant `docs/plan/*`.
- Edit only owned paths. Commit **only the explicit paths you changed** with `type(scope): summary` + `Refs #N`, then `git push origin main` (approved 2026-10-31; matches AGENTS.md rule 3). Never `git add -A`, never `reset`/`stash`/`checkout`/`restore`/`rebase`/`rm`, and never delete files you did not create; the coordinator keeps the board and never commits a sibling's paths.
- Run formatters/linters only for your own crate/package; the orchestrator runs workspace gates once per wave.
- Report: files changed, commands run with real output, anything not done and why. Unfinished work is reported, never hidden.
- Findings that change scope: comment on the issue / open a new one via `scripts/board.sh new`; do not silently expand scope.

Orchestrator checklist per wave: create/refresh issues -> mark `Ready` -> dispatch (set `In progress`) -> verify (gates) -> confirm the owner's commit/push carries `Refs #N` -> `board.sh done` with sha -> post gate comment.

## Parallelization map

Waves are defined by contract freezes, not by calendar: P1a (contracts) -> P1b (core streams + UI streams in parallel against the frozen IPC/mock) -> P1c (integration). Each stream owns a module/dir listed in `docs/plan/06-ipc-contract.md` (ownership map) so parallel agents never edit the same file.

User-authored files (e.g. `docs/WORKFLOW.md`) are never modified or removed without asking.
