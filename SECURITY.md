# Security Policy

## Supported Versions

| Version | Supported |
|---|---|
| 2.0.x (latest) | ✅ Active |
| 1.x | ✅ Security fixes only |
| 0.x | ❌ Unsupported |

## Reporting a Vulnerability

**Do not open a public issue.** Email security disclosures to:

**perseus@perseus.observer**

You will receive a response within 48 hours. Perseus Computing LLC is a US-owned
small business and treats security reports as confidential until a fix is published.

### What to include

- Affected version(s) and build target (Linux, macOS, Windows)
- Steps to reproduce
- Impact assessment (what an attacker could do)
- Any suggested mitigations

### Disclosure timeline

1. **Acknowledgment** — within 48 hours
2. **Triage** — severity assessment within 5 business days
3. **Fix development** — timeline depends on severity
4. **Coordinated disclosure** — CVE assigned, fix released, advisory published

We support responsible disclosure and will credit reporters who follow this policy.

---

## Security Model

Mimir is a **local-first MCP server** that stores AI agent memory. It processes:

- Entity CRUD (remember, recall, search, forget)
- Journaling (append-only decision logs)
- State management (key-value with TTL)
- Optional embeddings (Ollama / ONNX Runtime)
- Optional connectors (GitHub issues, file watcher)

### Encryption

Mimir supports **AES-256-GCM encryption at rest** for entity bodies.

| Property | Detail |
|---|---|
| Algorithm | AES-256-GCM |
| Key derivation | From user-provided passphrase |
| Encryption scope | Entity `body_json` field |
| Encrypted at rest | ✅ All stored entities |
| Encrypted in transit | ⚠️ MCP stdio transport (local only; HTTP transport uses TLS when configured) |
| Key management | User responsibility — keys never leave the deployment boundary |

**Enable encryption:**
```bash
mimir --encryption-key "your-strong-passphrase"
```

### Attack surface

| Vector | Risk | Mitigation |
|---|---|---|
| SQL injection | None | Parameterized queries via rusqlite — no string concatenation |
| Malicious MCP requests | Low | JSON-RPC 2.0 validation; MCP stdio is local-only by default |
| Entity injection (FTS5) | Low | FTS5 uses parameterized queries; inputs are escaped |
| File watcher path traversal | Medium | Paths are canonicalized before watching; only configured directories |
| GitHub connector token exposure | Medium | Token is never logged or stored in the database; memory-only during connector run |
| Embedding model download | Low | Optional; models are downloaded from Ollama or ONNX Runtime's official CDN |
| HTTP transport (axum) | Medium | CORS configured; no authentication by default (local-only intended use) |

### Trust boundaries

- **Mimir runs on your machine.** It does not phone home. No telemetry.
- **MCP transport is local stdio by default.** No network exposure unless you enable HTTP transport.
- **Connectors are opt-in.** GitHub and file watcher connectors are disabled by default.
- **Encryption keys are your responsibility.** Mimir does not store, transmit, or escrow keys.

---

## Compliance

| Standard | Status |
|---|---|
| NIST SP 800-53 | Mapping in progress |
| NIST AI RMF | Alignment documented |
| EO 14028 (SBOM) | [SBOM published](./docs/SBOM.md) |
| CMMC Level 2 | In progress — encryption, access control, audit trail |
| ITAR | US-owned LLC; all development in US; no foreign nationals on codebase |

---

## Dependency Security

- **17 runtime dependencies** — all MIT or Apache-2.0 licensed
- **Zero copyleft (GPL/AGPL)** — safe for government deployment
- **SQLite bundled** via rusqlite — no system library dependency
- **SBOM published** at [docs/SBOM.md](./docs/SBOM.md)
- We monitor [RustSec Advisory Database](https://rustsec.org) for crate CVEs
- `cargo audit` run in CI on every push

---

## Contact

Security: **perseus@perseus.observer**
PGP key: Available on request
