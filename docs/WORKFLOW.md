# Alloy Developer Workflow: Hindsight Memory

**Goal:** Use **Hindsight memory** (project context, decisions, past sessions) as the persistent knowledge layer, and ordinary repository tools (`rg`, `read`, `git`, Cargo) for live code navigation.

---

## Quick Reference

| Need | Tool | Command |
|------|------|---------|
| **What did we decide about X?** | Hindsight | `hindsight_search_knowledge_pages(query="X")` |
| **Why did we choose Y?** | Hindsight | `hindsight_reflect(query="why Y")` |
| **Where is symbol F defined?** | Repo | `rg -n "fn F\b|struct F\b|enum F\b"` |
| **Who calls F?** | Repo | `rg -n "\bF\s*\("` |
| **What changed in this commit?** | Git | `git diff --stat` + `git diff` |
| **Architecture overview** | Repo | `rg --files crates apps` + `docs/PLAN.md` |
| **Store a finding/decision** | Hindsight | `hindsight_ingest_document(title="...", content="...")` |
| **Track multi-session work** | Hindsight | `hindsight_capture_initiative(title="...", summary="...")` |

---

## Configuration

### Hindsight Bank: `omp-alloy`

**Location:** `~/.hindsight/coding-agent.json`

```json
{
  "serverMode": "self-hosted",
  "apiUrl": "https://api.hindsight.hce.vn/",
  "mapPathToBank": {
    "/Users/cento/git/personal/alloy": "omp-alloy"
  }
}
```

**Auto-ingestion:**
- **Git commits** (recent): seeded on first session; topped up on every session start
- **Conversation**: captured automatically at session end
- **Codebase survey**: runs on first open, refreshes periodically; surfaces to "🛰️ researching…" then "✅ completed"

---

## Typical Session Flow

### 1. Start a Session

Hindsight injects a deep memory synthesis into context automatically. It includes:
- Recent project decisions from commits
- Key architecture facts from the codebase survey
- Related findings from past sessions

**Credit:** start relevant answers with `🧠 From Hindsight memory (<page>): …`

### 2. Explore Code

**Example:** "I need to understand how the timeline handles scroll events."

```
1. rg -n "onScroll|scroll" apps/alloy/ui/src
   → locate scroll-handling components and handlers

2. read the matching files with explicit line ranges
   → read only the symbol region, not the whole file

3. rg -n "onScroll" apps/alloy/ui crates
   → see every call site (virtua, state updates, Tauri IPC)
```

Start with `rg --files` or a directory listing when the location is unknown; use `read` with line ranges once the symbol is found.

### 3. Understand Decisions (Hindsight)

**Example:** "Why did we choose Virtua for timeline?"

```
hindsight_search_knowledge_pages(query="Virtua timeline virtualization")
→ memory returns knowledge pages with evidence links, decision rationale

hindsight_reflect(query="why Virtua over Flutter timeline")
→ synthesizes deep reasoning across all stored facts
```

### 4. Make Changes

**Repository-guided refactoring:**
1. `git diff --stat` → list files you touched
2. `rg -n "<symbol>"` for every crate/app → find all references
3. Edit source, then verify with `cargo check --workspace` and the narrowest relevant test

**Example:** renaming a protocol command
```
1. git diff
   → identifies the protocol command definition

2. rg -n "CommandName" crates apps
   → find every reference (UI, Core, tests)

3. rg -n "CommandName" | rg -v "target/"
   → confirm handlers, dispatch, and UI callers are all updated

4. cargo check --workspace && cargo test -p alloy-protocol -p alloy-core
   → prove the rename compiles and callers still pass
```

### 5. Store Findings (Hindsight)

**When to store:**
- A **decision** reached in this session (new spike outcome, design choice)
- An **architecture fact** discovered (coupling, refactoring opportunity)
- A **non-obvious fix** (workaround, gotcha, bug pattern)

**How:**
```
hindsight_ingest_document(
  title="Decision: Virtua shift prop for scroll anchoring",
  content=`
# Why Virtua's shift=true works for reverse scroll

Virtua preserves scroll offset when prepending items by adjusting
the container's scrollTop. Confirmed in measure-scroll-perf.mjs
spike UI-04, achieving 55+ fps on low-end Android.

Gotcha: shift is only respected for prepend; append scrolls normally
(expected behavior for infinite-scroll chat).

References:
- apps/alloy/ui/scripts/measure-scroll-perf.mjs:295
- apps/alloy/ui/src/lib/components/Timeline.svelte
  `
)
```

The document is immediately discoverable via `hindsight_search_knowledge_pages`.

### 6. Correct Stale Memory

If Hindsight serves a fact that's now outdated:

```
hindsight_ingest_document(
  title="Correction: iOS support timeline",
  content=`
# iOS Support: Now Deferred to P5

Earlier memory said: "iOS in P2"

Actual decision (PLAN.md D10, 2026-10-16):
- Skip iOS in first versions; ship desktop + Android only
- iOS moves to P5 (post-1.0)
- Rationale: avoid app-store publishing delays, no WKWebView perf risk

Evidence: docs/PLAN.md line 19
  `
)
```

Newer facts **outrank** older ones in retrieval.

---

## Coordination: Hindsight ↔ Repository Search

### For refactoring across crates

**Scenario:** Move a type from `alloy-protocol` → `alloy-core` (it's only used in core).

1. **Hindsight:** Search for any **policy** on type ownership
   `hindsight_search_knowledge_pages(query="protocol contract API boundary")`

2. **Repo:** Find all references
   `rg -n "\bType\b" crates apps` → every importer, re-export, and test

3. **Verify**
   `cargo check --workspace && cargo test -p alloy-protocol -p alloy-core`

4. **Store the outcome**
   `hindsight_ingest_document(title="Refactor: Type moved protocol→core", content="...")`

### For impact analysis

**Scenario:** Spike S3 changed Android safe-area layout. What UI breaks?

1. **Repo:** Find all safe-area references
   `rg -n "safeArea|safe_area|safe-area" apps crates`

2. **Trace callers** with `rg` on each symbol, then read the narrow ranges.

3. **Hindsight:** Check if any component has **documented safe-area assumptions**
   `hindsight_search_knowledge_pages(query="safe area inset layout assumption")`

4. **Write smoke tests** for affected components, then commit.

---

## Multi-Session Development (Spike Planning)

When planning a spike (e.g., S4 — storage + keychain):

1. **Read PLAN.md** to understand the spike's contract
2. **Search Hindsight** for any prior spike attempts
   `hindsight_search_knowledge_pages(query="S4 storage spike")`
3. **Map the spike's scope**
   `rg --files crates/alloy-storage crates/alloy-core apps/alloy` + read their `lib.rs`/entry points
4. **Create an initiative** to track work across sessions
   `hindsight_capture_initiative(title="Spike S4: Storage + Keychain", summary="...")`
5. After the spike, store the outcome
   `hindsight_ingest_document(title="Spike S4 Complete: …", content="…")`

---

## Common Queries

### "Show me the architecture"
```
rg --files crates apps | sed 's#/[^/]*$##' | sort -u
→ module and package layout

rg -n "^name\s*=" crates/*/Cargo.toml apps/*/Cargo.toml
→ workspace crate/app names

read docs/PLAN.md and the relevant docs/conventions file
→ intended layers: UI, API, Core, Storage
```

### "What's high-risk in this commit?"
```
git diff --stat
→ files touched

git diff -U0
→ changed symbols and lines

rg -n "<changed symbol>" crates apps
→ all call sites

cargo check --workspace && cargo test -p <affected crate>
→ prove the change did not break callers
```

### "Find dead code"
```
cargo clippy --workspace --all-targets -- -W dead_code
→ compiler/Clap warnings for unused items

rg -n "<candidate>" crates apps
→ confirm no dynamic/string-based use before deleting
```

### "How does the UI send a command to Core?"
```
1. rg -n "enum Command|struct Command" crates
2. rg -n "\bCommand::" apps crates
   → UI dispatch sites and Core handlers
3. read the matching protocol and dispatch files
```

---

## Checklist: Efficient Workflow

- [ ] **Session start:** Hindsight injects memory → read context summary
- [ ] **Exploring code:** Locate with `rg --files` / `rg -n`, then `read` explicit ranges
- [ ] **Decision questions:** `hindsight_reflect()` for synthesis, `hindsight_search_knowledge_pages()` for facts
- [ ] **After changes:** `git diff` → `rg` all references → `cargo check`/tests
- [ ] **Non-obvious fixes:** Store in Hindsight for next session
- [ ] **Stale memory spotted:** Correct it immediately with `hindsight_ingest_document("Correction: ...")`
- [ ] **Spike work:** Create initiative, then store outcome at completion

---

## Links

- **PLAN.md** — master plan, phases, gates, spike definitions
- **Hindsight skill** → `skill://hindsight-coding-agent`
