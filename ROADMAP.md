# Mneme Roadmap

## What Mneme Is

A local-first persistent memory engine for AI agents. MCP-native. Single static binary. Zero runtime dependencies. Memories stored as `.md` files in a vault — human-readable, git-trackable, Obsidian-compatible.

## What Mneme Is Not

- Not a knowledge graph or entity extraction engine
- Not a cloud service or SaaS
- Not a replacement for a vector database
- Not dependent on any specific AI assistant or framework

---

## v0.1 — MVP (Current)

**Status:** ✅ Shipped

- SQLite + FTS5 keyword search with LIKE fallback
- MCP JSON-RPC 2.0 stdio server
- Three tools: `mneme_store`, `mneme_recall`, `mneme_health`
- Single static binary, bundled SQLite, zero runtime deps
- Bootstrap script for one-command install
- Works fully offline after build

**What's good:** The foundation is solid. FTS5 is fast and zero-cost. The MCP interface works with any host.

**What's missing:** Keyword search is the floor, not the ceiling. No memory management (accumulate forever). No embedding model or vector path. No `.md` vault — memories live only in SQLite.

---

## v0.2 — The `.md` Vault
**Target:** "Your memories are your files."

This is the release that makes Mneme a real standalone tool with a clear differentiator.

### `.md` vault storage
Memories stored as individual `.md` files with YAML frontmatter in `~/.mneme/vault/`. The SQLite database becomes a search index over the vault, not the source of truth.

```
~/.mneme/vault/
├── mem-2026-06-09-a1b2c3.md
├── mem-2026-06-09-d4e5f6.md
└── mem-2026-06-10-g7h8i9.md
```

Each file:
```markdown
---
id: mem-a1b2c3d4e5f6
type: architecture
tags: [database, postgres]
created: 2026-06-09T14:22:00Z
importance: 0.8
---
The project uses PostgreSQL 16 with connection pooling via pgbouncer.
Port 5432, max 100 connections. Auth via scram-sha-256.
```

### Why `.md` vault?

| Property | Benefit |
|----------|---------|
| Human-readable | Open in any editor, not a black box |
| Git-trackable | Version your agent's memory like code |
| Obsidian-compatible | Drop the vault into Obsidian, browse memories visually |
| Portable | Move between tools, machines, backup with rsync |
| Agent-editable | Agents can read/write `.md` as easily as calling an API |
| Diffable | See what your agent learned between sessions |

### MCP tool: `mneme_vault_export`
Export all memories to `.md` files (idempotent — updates changed files, creates new ones, never deletes).

### MCP tool: `mneme_vault_import`
Scan the vault directory and index new/changed `.md` files into SQLite FTS5.

### FTS5 index rebuild
Rebuild the search index from the vault on demand. Makes the vault the canonical store and SQLite a performance cache.

### Existing tools work unchanged
`mneme_store` still writes to SQLite. `mneme_vault_export` syncs to `.md`. If someone only ever calls `mneme_store`/`mneme_recall`, nothing changes.

---

## v0.3 — Memory Management
**Target:** "Your agent's memory doesn't rot."

Memories currently accumulate forever with no decay, dedup, or summarization. This release adds basic memory hygiene.

### Ebbinghaus decay
Memories lose relevance over time based on a forgetting curve. `decay_score` drops when a memory isn't retrieved. `mneme_recall` filters out memories below a threshold by default. Retrieval resets the decay clock.

Implementation: a `decay_score` column already exists (always `1.0` in v0.1). A background pass updates scores on recall, and a configurable threshold drops stale memories from results.

### Near-duplicate detection
When storing a new memory, check existing memories for high similarity (Levenshtein or trigram overlap on content). If a near-duplicate exists, bump its importance and skip the store. Avoids the "agent stores the same fact 40 times" problem.

### Configurable TTL
Memories can be tagged with a TTL at store time: "this debugging session fact expires in 24 hours." The decay algorithm respects TTL alongside the forgetting curve.

### Memory compaction
A manual `mneme_compact` tool that:
- Merges near-duplicate memories into single entries
- Summarizes clusters of related low-importance memories
- Drops memories below a configurable decay threshold
- Rebuilds the vault from the compacted state

### Memory stats
`mneme_health` extended to report:
- Total memories, by type, by age bucket
- Decay distribution
- Storage size (SQLite + vault)
- Recall hit rate (how often queries return results)

---

## v0.4 — Semantic Search
**Target:** "Find what I mean, not what I typed."

FTS5 keyword search requires exact word matches. This release adds a vector path for semantic similarity.

### Embedding support
- Bundled small embedding model (all-MiniLM-L6-v2 via `ort` or `candle`) — no external service, no API key
- Optional: point at any OpenAI-compatible embedding endpoint for better quality
- Embeddings stored alongside memories in SQLite
- Hybrid search: FTS5 keyword + cosine similarity, merged and ranked

### Why bundled, not external?
Mneme's pitch is "works offline." Requiring Ollama or an API key for embeddings breaks that. A small ONNX model (~80MB) bundled at build time keeps the offline guarantee. Quality is lower than a cloud embedding, but for memory retrieval ("find my notes about database config"), it's more than sufficient.

### MCP tool: `mneme_search`
Replaces `mneme_recall` with a unified search that combines keyword + semantic. Accepts a `mode` parameter: `keyword`, `semantic`, or `hybrid` (default).

### Backward compatibility
`mneme_recall` continues to work — it becomes an alias for `mneme_search` with `mode=keyword`. No breaking change.

---

## v0.5 — Multi-Agent & Federation
**Target:** "One memory engine, many agents, many workspaces."

### Workspace scoping
Memories tagged with `workspace` at store time. Recall can filter by workspace or search across all. Different projects don't pollute each other's memory.

### Agent identity
Memories tagged with the agent that stored them. Recall can filter by agent. An agent can ask "what did I learn last time?" or "what did any agent learn about this repo?"

### Cross-workspace federation
Two Mneme instances on different machines can share memories. Not real-time sync — export/import via the vault directory. Rsync your `~/.mneme/vault/` between machines, rebuild the index, and your laptop agent recalls what your desktop agent learned.

### MCP tool: `mneme_federate`
Push/pull memories between instances via the vault. `mneme_federate pull` scans a remote vault path and imports new memories. `mneme_federate push` exports local memories to a target vault.

---

## v0.6 — Memory Synthesis
**Target:** "Summarize what we know."

### Topic clustering
Group memories by topic using the embedding model. Surface clusters of related memories that aren't explicitly linked.

### Memory chains
When an agent stores linked memories (via the `links` field that already exists in the schema), `mneme_recall` can traverse the chain. "Show me everything related to this architecture decision" follows the link graph.

### Auto-summarization
`mneme_summarize` tool: given a topic path or query, returns a synthesized summary of all matching memories. Uses a local LLM if available (via Ollama endpoint), falls back to concatenation if not.

### Memory timeline
`mneme_timeline` tool: returns memories in chronological order, optionally filtered by topic. "What did we learn about the database between March and May?"

---

## Beyond v0.6 — Ideas, Not Commitments

### SSE/HTTP transport
Serve Mneme over HTTP + SSE for non-stdio MCP hosts. Useful for browser-based agents and remote setups.

### Obsidian plugin
A community plugin that treats Mneme's vault as an Obsidian vault. Browse, search, and edit agent memories in Obsidian. Close the loop: you write notes in Obsidian, your agent learns from them via Mneme, the agent's learnings appear back in Obsidian.

### Memory quality scoring
Agents can rate memories: "this fact was useful" or "this was wrong." Quality scores feed into recall ranking and decay.

### Memory conflict resolution
When two agents store contradictory facts about the same topic, flag the conflict for human review.

### Audit log
Every memory operation logged to a journal. "Who stored this fact and when?" Useful for debugging agent behavior over time.

---

## Design Principles

These are non-negotiable and apply to every release:

1. **Zero runtime dependencies.** The binary is self-contained. No Python, no Node, no Docker. Just a static binary and a directory of `.md` files.

2. **Offline-first.** All core operations work without internet. Optional cloud features (federation, remote embeddings) are opt-in, not required.

3. **Human-readable storage.** The vault is the source of truth. If Mneme disappears tomorrow, your memories are still `.md` files you can read with any text editor.

4. **MCP-native.** Every feature ships as an MCP tool. No SDK to install, no library to import. Any MCP host can use Mneme immediately.

5. **Agent-first, not human-first.** Tools are designed for AI agents to call programmatically. The human UX is the `.md` vault — open it in any editor.

6. **Compose, don't integrate.** Mneme does one thing (persistent memory). It composes with other tools (Perseus for context, Obsidian for browsing, Git for versioning) rather than trying to absorb their functionality.

---

## Competitive Positioning

Mneme sits in the **lightweight / zero-dependency** tier of AI memory tools, alongside Mnemosyne and Holographic. Its differentiators:

| Dimension | Mneme | Mem0 | Hindsight |
|-----------|-------|------|-----------|
| Runtime deps | Static binary | Python/cloud | PostgreSQL |
| Storage format | `.md` vault | Managed DB | PostgreSQL |
| MCP-native | ✅ day one | Via wrapper | Via plugin |
| Offline | ✅ fully | ❌ cloud | Partial |
| Setup time | 1 command | 30 seconds | Hours |
| LLM required | ❌ none | ✅ for extraction | ✅ for synthesis |
| Cost | $0 forever | Freemium | Free (local) |
| Human-readable | ✅ `.md` files | ❌ opaque | ❌ SQL |

**When to choose Mneme:**
- You want agent memory that lives in files you can read
- You need fully offline operation
- You're already using MCP-compatible tools
- You want zero recurring cost and zero API dependencies
- Your memory needs are factual recall, not knowledge graph construction

**When to choose Hindsight instead:**
- You need entity extraction and knowledge graphs
- You need automatic memory capture during conversation
- You're willing to run PostgreSQL

**When to choose Mem0 instead:**
- You want 30-second setup with no config
- You're fine with cloud dependencies
- You need user-level scope across sessions and agents
