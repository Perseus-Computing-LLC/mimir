# Mimir Roadmap

## What Mimir Is

A local-first persistent memory engine for AI agents. MCP-native. Single static binary.
Zero runtime dependencies. Structured entity model with journal events and state management.

## What Mimir Is Not

- Not a knowledge graph or entity extraction engine
- Not a cloud service or SaaS
- Not a replacement for a vector database
- Not dependent on any specific AI assistant or framework

---

## v0.1 — MVP

**Status:** ✅ Shipped (2026-05)

- SQLite + FTS5 keyword search with LIKE fallback
- MCP JSON-RPC 2.0 stdio server
- Three tools: `mimir_store`, `mimir_recall`, `mimir_health`
- Single static binary, bundled SQLite, zero runtime deps

---

## v0.2.0 — Structured Entity Model

**Status:** ✅ Shipped (2026-06-10)

This release makes Mimir competitive with structured memory systems (Sibyl, Mem0)
by adding an entity model with composite keys, journal events, and state management.

### Three-table schema
- **entities** — idempotent by UNIQUE(category, key), FTS5-indexed
- **journal** — append-only event log with evaluated/acted/forward structure
- **state** — key-value with optional TTL and auto-expiration

### Entity tools
- `mimir_remember` — idempotent entity upsert by (category, key)
- `mimir_recall` — FTS5 search with category, type, topic, decay filters
- `mimir_forget` — soft-delete (archived=1) with reason
- `mimir_link` / `mimir_unlink` — entity relationship graph

### Journal tools
- `mimir_journal` — append structured events (decision/observation/action)
- `mimir_timeline` — time-range query with category/type/entity filters

### State tools
- `mimir_state_set` — key-value with optional TTL
- `mimir_state_get` — retrieve with auto-expiration check
- `mimir_state_delete` / `mimir_state_list` — management

### Management
- `mimir_stats` — full statistics across all three tables
- `mimir_compact` — archive entities below decay threshold
- `mimir_migrate` — CLI subcommand for v0.1.x → v0.2.0 migration
- `mimir_context` — pre-formatted markdown context block for session injection
- `mimir_workspace_list` — list all distinct categories

### Perseus integration
- Rewrote `mimir_connector.py` for entity model
- Removed Sibyl Memory dependency entirely
- Mimir is now the sole persistent memory backend for Perseus

---

## v0.2.1 — Decay & Layers

**Target:** "Memories that get stronger with use, fade with neglect."

### Ebbinghaus decay algorithm
The `decay_score` column already exists (always 1.0 today). v0.2.1 wires up
actual decay: scores drop over time when memories aren't retrieved, reset on
recall. Low-decay entities can be auto-archived.

### Layer progression
Entities progress through buffer → working → core based on retrieval patterns.
Auto-promotion when retrieval_count crosses thresholds. Core entities are
permanent; buffer entities are volatile.

### Near-duplicate detection
Trigram or Levenshtein similarity check on body_json at store time. Bump
importance on existing entity instead of creating near-duplicates.

---

## v0.3 — Semantic Search + `.md` Vault

**Target:** "Find what I mean, not what I typed."

### Embedding-based vector search
- Bundled small embedding model (all-MiniLM-L6-v2 via `ort` or `candle`)
- Optional: point at any OpenAI-compatible embedding endpoint
- Hybrid search: FTS5 keyword + cosine similarity, merged and ranked
- New `mimir_search` tool with `mode` parameter: keyword, semantic, hybrid

### `.md` vault storage
Memories stored as individual `.md` files with YAML frontmatter in `~/.mimir/vault/`.
SQLite becomes a search index, not the source of truth.

```
~/.mimir/vault/
├── mem-2026-06-09-a1b2c3.md
└── mem-2026-06-10-d4e5f6.md
```

- Human-readable, git-trackable, Obsidian-compatible
- `mimir_vault_export` / `mimir_vault_import` tools
- Existing tools work unchanged (store writes to SQLite, export syncs to `.md`)

---

## v0.4 — Multi-Agent & Federation

**Target:** "One memory engine, many agents, many workspaces."

- Workspace scoping with `workspace_hash`
- Agent identity tracking on stored memories
- Cross-workspace federation via vault sync
- SSE/HTTP transport for non-stdio MCP hosts

---

## v0.5 — Memory Synthesis

**Target:** "Summarize what we know."

- Topic clustering using embedding model
- Memory chain traversal via entity links
- Auto-summarization of topic clusters
- Memory quality scoring (agents rate memories)
- Conflict detection (contradictory facts flagged)

---

## Design Principles

1. **Zero runtime dependencies.** The binary is self-contained.
2. **Offline-first.** All core operations work without internet.
3. **MCP-native.** Every feature ships as an MCP tool.
4. **Agent-first, not human-first.** Tools are designed for AI agents.
5. **Compose, don't integrate.** Mimir does persistent memory; composes with Perseus, Obsidian, Git.
