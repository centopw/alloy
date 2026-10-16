# Alloy Developer Workflow: Hindsight + Codebase-Memory

**Goal:** Combine **Hindsight memory** (project context, decisions, past sessions) with **codebase-memory-mcp** (live code graph) for fast, accurate development.

---

## Quick Reference

| Need | Tool | Command |
|------|------|---------|
| **What did we decide about X?** | Hindsight | `hindsight_search_knowledge_pages(query="X")` |
| **Why did we choose Y?** | Hindsight | `hindsight_reflect(query="why Y")` |
| **Who calls function F?** | Codebase | `trace_path(function_name="F", direction="inbound")` |
| **What does F call?** | Codebase | `trace_path(function_name="F", direction="outbound")` |
| **Find dead code** | Codebase | `search_graph(max_degree=0, exclude_entry_points=true)` |
| **Architecture overview** | Codebase | `get_architecture(path="crates/")` |
| **Store a finding/decision** | Hindsight | `hindsight_ingest_document(title="...", content="...")` |
| **Commit affects what?** | Codebase | `detect_changes()` + `trace_path()` |

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

### Codebase-Memory Project: `alloy`

**Status:** Indexed and ready.  
**Verify:** `xd://mcp__codebase_memory_mcp_index_status` → check `status: "ready"`

**Known parse gaps** (best-effort indexed; use grep for these ranges):
- `apps/alloy/ui/scripts/measure-scroll-perf.mjs` lines 425, 428, 432 (template literal with nested backticks)
- `apps/alloy/ui/src/lib/components/Hud.svelte` line 85 (multi-directive class binding)

---

## Typical Session Flow

### 1. Start a Session
Hindsight injects a deep memory synthesis into context automatically. It includes:
- Recent project decisions from commits
- Key architecture facts from codebase survey
- Related findings from past sessions

**Credit:** start relevant answers with `🧠 From Hindsight memory (<page>): …`

### 2. Explore Code (Codebase-Memory First)

**Example:** "I need to understand how the timeline handles scroll events."

```
1. search_graph(name_pattern=".*[Ss]croll.*", label="Function")
   → find scroll-handling functions

2. get_code_snippet(qualified_name="ui.routes.Timeline.onScroll")
   → read the implementation

3. trace_path(function_name="onScroll", direction="outbound", depth=2)
   → see what it calls (virtua, state updates)

4. trace_path(function_name="onScroll", direction="inbound")
   → see what triggers it (event listeners, Tauri IPC handlers)
```

**Performance:** Graph queries return in ~500 tokens. Grep equivalent: ~80K tokens.

### 3. Understand Decisions (Hindsight)

**Example:** "Why did we choose Virtua for timeline?"

```
hindsight_search_knowledge_pages(query="Virtua timeline virtualization")
→ memory returns knowledge pages with evidence links, decision rationale

hindsight_reflect(query="why Virtua over Flutter timeline")
→ synthesizes deep reasoning across all stored facts
```

### 4. Make Changes

**Graph-guided refactoring:**
1. `detect_changes()` → map your commit to affected symbols
2. `trace_path()` on those symbols → find every callsite
3. Edit source + verify coverage with `check_index_coverage()`

**Example:** renaming a protocol command
```
1. detect_changes() on your git diff
   → identifies alloy-protocol::{CommandName}

2. search_graph(name_pattern="CommandName")
   → find all references (UI, Core, tests)

3. trace_path(symbol="CommandName", direction="both")
   → ensure you've updated handlers, dispatch, UI callers

4. check_index_coverage(paths=["alloy-protocol", "alloy-core", "apps/alloy/ui"])
   → verify the index knows about your files
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

## Coordination: Hindsight ↔ Codebase-Memory

### For refactoring across crates

**Scenario:** Move a type from `alloy-protocol` → `alloy-core` (it's only used in core).

1. **Hindsight:** Search for any **policy** on type ownership  
   `hindsight_search_knowledge_pages(query="protocol contract API boundary")`

2. **Codebase:** Trace all references  
   `trace_path(symbol="Type", direction="both")` → find every importer

3. **Verify coverage**  
   `check_index_coverage(paths=["alloy-protocol", "alloy-core", "apps/alloy/ui/src/lib/generated"])`

4. **Store the outcome**  
   `hindsight_ingest_document(title="Refactor: Type moved protocol→core", content="...")`

### For impact analysis

**Scenario:** Spike S3 changed Android safe-area layout. What UI breaks?

1. **Codebase:** Find all safe-area references  
   `search_graph(name_pattern=".*safeArea.*", label="Function")`

2. **Trace callers**  
   `trace_path(function_name="applySafeArea", direction="inbound")`

3. **Hindsight:** Check if any component has **documented safe-area assumptions**  
   `hindsight_search_knowledge_pages(query="safe area inset layout assumption")`

4. **Write smoke tests** for affected components, then commit

---

## Multi-Session Development (Spike Planning)

When planning a spike (e.g., S4 — storage + keychain):

1. **Read PLAN.md** to understand the spike's contract
2. **Search Hindsight** for any prior spike attempts  
   `hindsight_search_knowledge_pages(query="S4 storage spike")`
3. **Graph the spike's scope**  
   `get_architecture(path="crates/alloy-storage")` + `get_architecture(path="crates/alloy-core")`
4. **Create an initiative** to track work across sessions  
   `hindsight_capture_initiative(title="Spike S4: Storage + Keychain", summary="...")`
5. After the spike, store the outcome  
   `hindsight_ingest_document(title="Spike S4 Complete: …", content="…")`

---

## Common Queries

### "Show me the architecture"
```
get_architecture(request="structure")
→ counts, languages, entry points

get_architecture(request="dependencies")
→ internal + external deps

get_architecture(request="layers")
→ tiers: UI, API, Core, Storage
```

### "What's high-risk in this commit?"
```
detect_changes()
→ affected symbols

trace_path(function_name=SYMBOL, direction="both", risk_labels=true)
→ high-risk callsites highlighted
```

### "Find dead code"
```
search_graph(max_degree=0, exclude_entry_points=true)
→ unreachable functions (test only if graph is current)

check_index_coverage(paths=["..."])
→ verify no gaps before claiming dead code
```

### "How does the UI send a command to Core?"
```
1. search_graph(name_pattern=".*Command.*", label="Type")
2. get_code_snippet(qualified_name="alloy_protocol::Command")
3. trace_path(symbol="Command", direction="inbound")
   → UI dispatch sites
4. trace_path(symbol="Command", direction="outbound")
   → Core handlers
```

---

## Checklist: Efficient Workflow

- [ ] **Session start:** Hindsight injects memory → read context summary
- [ ] **Exploring code:** Use graph (`trace_path`, `search_graph`) before grep
- [ ] **Decision questions:** `hindsight_reflect()` for synthesis, `search_knowledge_pages()` for facts
- [ ] **After changes:** `detect_changes()` → `trace_path()` to verify scope
- [ ] **Non-obvious fixes:** Store in Hindsight for next session
- [ ] **Stale memory spotted:** Correct it immediately with `hindsight_ingest_document("Correction: ...")`
- [ ] **Spike work:** Create initiative, then store outcome at completion

---

## Links

- **PLAN.md** — master plan, phases, gates, spike definitions
- **Hindsight skill** → `skill://hindsight-coding-agent`
- **Codebase-Memory skill** → `skill://codebase-memory`
- **Codebase-Memory MCP** → `xd://mcp__codebase_memory_mcp_*` tools
