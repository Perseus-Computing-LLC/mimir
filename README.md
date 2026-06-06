# engram-rs

Persistent memory engine for AI coding assistants. Hybrid keyword search over
SQLite + FTS5. MCP-native — drop-in memory for any MCP host. Works standalone:
no API keys, no embeddings, no LLM required.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://rust-lang.org)

---

## ⚡ One-Shot Bootstrap

A single command that installs Rust, builds engram-rs, creates the database, and
verifies everything. **Paste this into any terminal — an AI assistant can run it
with zero context:**

```bash
curl -sSL https://raw.githubusercontent.com/tcconnally/engram-rs/main/scripts/bootstrap.sh | bash
```

What it does:
1. Installs system build tools (gcc, pkg-config) if missing
2. Installs Rust via rustup if not present
3. Clones and builds engram-rs from source (`cargo build --release`)
4. Installs the binary to `~/.local/bin/engram`
5. Creates the data directory `~/.perseus/engram/` and warms up the SQLite database
6. Creates/amends `.env` with `ENGRAM_DB_PATH`
7. Smoke-tests the MCP server
8. Prints a success summary

**Idempotent** — safe to re-run. Set `FORCE=1` to force a rebuild.

---

## What It Does

Engram-rs is a lightweight **MCP JSON-RPC 2.0 stdio server** that provides
durable memory for AI agents. It stores, searches, and retrieves memories
using SQLite with full-text search (FTS5).

### MCP Tools

| Tool | Description |
|------|-------------|
| `engram_store` | Store a memory with content, type (`insight`/`architecture`/`decision`), tags, and importance |
| `engram_recall` | Search memories by keyword query (FTS5 + LIKE fallback), filtered by type, workspace, topic |
| `engram_health` | Check server and database health |

### Key Properties

- **Zero dependencies at runtime** — static binary with bundled SQLite, no network needed
- **Keyword search** — FTS5 for BM25-ranked results, LIKE fallback for multi-word queries
- **No LLM required** — stores and retrieves memories directly; no fact extraction, no embeddings
- **MCP-native** — standard JSON-RPC 2.0 over stdio; works with any MCP host (Claude Desktop, Hermes Agent, etc.)
- **Single-file database** — one SQLite file with FTS5 index; easy to backup, copy, or inspect

---

## Usage

### Standalone

```bash
# Start the MCP server
engram serve --db ~/.perseus/engram/engram.db --mcp

# Show version
engram --version
```

### With Perseus

Add to `.perseus/config.yaml`:

```yaml
engram:
  enabled: true
  transport: "stdio"
  command: ["engram", "serve", "--db", "~/.perseus/engram/engram.db", "--mcp"]
  timeout_s: 10.0
  merge_strategy: "local_first"
  fallback_to_local: true
  circuit_breaker:
    threshold: 3
    cooldown: 120
```

Then add the `@memory` directive to `.perseus/context.md`:

```markdown
## Long-Term Memory (Engram-rs)

@memory workspace_hash="auto" max_results=5 focus=recent
```

Perseus will automatically call `engram_recall` at render time and populate the
`AGENTS.md` context with relevant memories.

### Manual MCP Testing

```bash
# Start the server
engram serve --db /tmp/test.db --mcp

# In another terminal, send JSON-RPC:
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | engram serve --db /tmp/test.db --mcp
```

---

## Building from Source

```bash
git clone https://github.com/tcconnally/engram-rs.git
cd engram-rs
cargo build --release
# Binary at target/release/engram
```

**Requirements:** Rust 1.70+ (stable), a C compiler (rusqlite bundles SQLite).

---

## Database Schema

```sql
CREATE TABLE memories (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    type TEXT NOT NULL DEFAULT 'insight',
    summary TEXT DEFAULT '',
    relevance REAL DEFAULT 0.0,
    decay_score REAL DEFAULT 1.0,
    retrieval_count INTEGER DEFAULT 0,
    layer TEXT DEFAULT 'working',
    topic_path TEXT DEFAULT '',
    created_at_unix_ms INTEGER NOT NULL,
    last_accessed_unix_ms INTEGER NOT NULL,
    workspace_hash TEXT DEFAULT '',
    tags TEXT DEFAULT '{}',
    links TEXT DEFAULT '[]',
    source TEXT DEFAULT 'engram',
    verified INTEGER DEFAULT 0
);

CREATE VIRTUAL TABLE memories_fts USING fts5(content, content_rowid='rowid');
```

---

## Version & Roadmap

**Current:** v0.1.0 — MVP

| Feature | Status |
|---------|--------|
| MCP JSON-RPC 2.0 stdio server | ✅ |
| Keyword search (FTS5 + LIKE) | ✅ |
| Memory store with metadata | ✅ |
| SQLite persistence | ✅ |
| Embedding-based vector search | 🔜 v0.2 |
| Ebbinghaus decay algorithm | 🔜 v0.2 |
| Cross-workspace federation | 🔜 v0.3 |
| SSE transport | 🔜 v0.3 |

---

## License

MIT — see [LICENSE](./LICENSE).
