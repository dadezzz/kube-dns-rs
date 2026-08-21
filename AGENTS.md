# AGENTS.md

This file provides guidance for AI agents working on this project.

## Project Overview

**kube-dns-rs** is a Kubernetes-aware DNS server written in Rust using the [hickory-server](https://crates.io/crates/hickory-server) crate. It listens on port 1053 (TCP + UDP), applies domain blocklists, resolves Kubernetes service names, and forwards unresolved queries to QUAD9 DNS over TLS.

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `hickory-server` | DNS server implementation (core) |
| `kube` / `k8s-openapi` | Kubernetes API client & types (feature-gated) |
| `tokio` | Async runtime |
| `reqwest` | HTTP client for fetching blocklists |
| `rayon` | Parallel iteration for blocklist lookups |
| `serde` / `serde_yaml` | Serialization |
| `rustls` | TLS (aws-lc-rs backend) |

## Architecture

The server uses Hickory's `Catalog` to chain zone handlers:

1. **BlockListZoneHandler** — First handler; checks if a queried domain is blocked or allowed. Returns `Empty` for blocked domains, `Skip` otherwise.
2. **KubernetesSvcZoneHandler** — (feature-gated) Resolves queries matching the cluster domain by watching Kubernetes services.
3. **ForwardZoneHandler** — Final fallback; forwards to QUAD9 DNS over TLS.

### Trie-Based Blocklisting

Blocklists are stored in a **trie** (prefix tree) for efficient domain matching. Each loaded URL produces one trie. Lookups are parallelized across all tries using `rayon::par_iter`. The resolution logic:

```
Allowed > Blocked > Neutral
Deepest prefix wins within each category
```

### Kubernetes Integration (feature-gated)

When the `kubernetes` feature is enabled:
- Two watcher modules exist: `kubernetes_svc` (Services) and `kubernetes_crd` (Custom Resources)
- Each module has: `context.rs` (shared state), `handler.rs` (ZoneHandler), `watcher.rs` (Kubernetes API watcher)
- The cluster domain is hardcoded: `svc.k8s.zarantonello.dev`

## Code Conventions

- **Edition**: Rust 2024
- **Async**: `tokio` runtime, `#[tokio::main]` for entry point
- **Error handling**: `thiserror` for custom error types
- **Feature flags**: `kubernetes` (default) controls Kubernetes-specific code behind `#[cfg(feature = "kubernetes")]`
- **Naming**: Snake case for functions/variables, PascalCase for types

## Building & Running

```bash
# Build
cargo build --release

# Run (requires kubeconfig for kubernetes feature)
./target/release/kube-dns-rs

# Build without Kubernetes
cargo build --release --no-default-features
```

## Testing

No test framework is currently configured. Tests would go in:
- Module-level `#[cfg(test)]` modules within each `.rs` file
- Integration tests in `tests/` directory

## CI/CD

- **Docker**: Multi-stage build using `git.zarantonello.dev/infra/ci-rust` base image
- **Releases**: Semantic release configured via `.releaserc.json`
- **Forgejo**: CI workflows in `.forgejo/workflows/`
- **Renovate**: Auto-dependency updates via `.renovaterc.json`

## Configuration Points (src/main.rs)

| Constant | Purpose |
|----------|---------|
| `BLOCKLIST_URLS` | URLs to fetch blocklists from |
| `ALLOWLIST_URLS` | URLs to fetch allowlists from |
| `cluster_domain` | Kubernetes cluster domain for service resolution |

## File Structure Reference

```
src/
├── main.rs                  # Entry: catalog setup, server binding, tokio::select loop
├── blocklist/
│   ├── domains.rs           # BlockListZoneHandlerDomains: fetch, parse, parallel lookup
│   ├── handler.rs           # BlockListZoneHandler: ZoneHandler impl
│   ├── trie.rs              # Trie: insert, find_closest_prefix
│   └── mod.rs
├── kubernetes_svc/          # Service discovery (watch + resolve)
│   ├── context.rs           # Shared RwLock<Context> holding service map
│   ├── handler.rs           # KubernetesSvcZoneHandler: ZoneHandler impl
│   └── watcher.rs           # KubernetesDnsWatcher: watches k8s API
└── kubernetes_crd/          # Custom resource discovery (watch + resolve)
    ├── context.rs
    ├── handler.rs
    └── watcher.rs
```

## Common Tasks

### Adding a New Blocklist

Edit `src/main.rs`:
```rust
const BLOCKLIST_URLS: [&str; 1] = [
    "https://example.com/blocklist.txt",
];
```

### Adding a New Zone Handler

1. Create a new module under `src/`
2. Implement the `hickory_server::zone_handler::ZoneHandler` trait
3. Register in `main.rs` with `catalog.upsert(origin, vec![Arc::new(handler)])`

### Adding Kubernetes Resources to Watch

1. Create a new module under `src/kubernetes_crd/` or `src/kubernetes_svc/`
2. Implement context, handler, and watcher following existing patterns
3. Register the watcher in `main.rs`
4. Register the handler in the catalog

## Important Notes

- The server blocks on `server.block_until_done()` — graceful shutdown via Ctrl+C calls `server.shutdown_gracefully().await`
- The `cfg_select!` macro is used to conditionally compile the Kubernetes watcher future (since `#[cfg]` doesn't work inside `tokio::select!`)
- Blocklist URLs are fetched eagerly at startup; no hot-reloading
- The trie uses `HashMap` for children — not the most cache-friendly but simple and correct
- QUAD9 is hardcoded as the upstream forwarder; to change, modify the `QUAD9` constant usage in `main.rs`
