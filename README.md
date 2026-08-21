# kube-dns-rs

A high-performance, Kubernetes-aware DNS server written in Rust.

## Features

- **Domain Blocklisting** — Fetch and apply blocklists from remote URLs using an efficient trie-based matching algorithm with parallel lookups
- **Kubernetes Service Discovery** — Watch and resolve Kubernetes services via Custom Resources and Service resources
- **DNS Forwarding** — Forward unresolved queries to upstream resolvers (QUAD9 DNS over TLS by default)
- **Allowlist Support** — Override blocklist entries with domain-specific allowlists
- **Container-Ready** — Optimized multi-stage Docker builds for minimal image size

## Architecture

```
Client Query
    │
    ▼
┌─────────────────────────┐
│   Zone Handler Catalog  │
├─────────────────────────┤
│  BlockListZoneHandler   │  ← Checks domain against block/allow lists
├─────────────────────────┤
│  KubernetesSvcHandler   │  ← Resolves k8s service names (feature-gated)
├─────────────────────────┤
│  ForwardZoneHandler     │  ← Forwards to QUAD9 DoT upstream
└─────────────────────────┘
```

### Blocklist System

Blocklists are loaded from URLs at startup. Each list is parsed into a **trie** data structure for efficient prefix matching. Lookups run in parallel across all loaded lists using `rayon`.

Domain status resolution:
1. **Allowed** overrides **Blocked** (allowlist wins)
2. **Blocked** overrides **Neutral**
3. Deepest matching prefix wins

### Kubernetes Integration

When the `kubernetes` feature is enabled (default), the server:
- Watches Kubernetes services and custom resources
- Resolves queries matching the configured cluster domain (`svc.k8s.zarantonello.dev`)
- Dynamically updates DNS records as services are created/removed

## Getting Started

### Prerequisites

- Rust 2024 edition toolchain
- Docker (for containerized builds)
- Kubernetes cluster with `kubeconfig` (for kubernetes feature)

### Build

```bash
# With Kubernetes support (default)
cargo build --release

# Without Kubernetes support
cargo build --release --no-default-features
```

### Run

```bash
# Default: listens on [::]:1053 (TCP + UDP)
./target/release/kube-dns-rs

# Without Kubernetes feature
KUBECONFIG="" ./target/release/kube-dns-rs
```

### Configuration

Edit `src/main.rs` to configure:

```rust
// Blocklist URLs (loaded at startup)
const BLOCKLIST_URLS: [&str; 0] = [
    // "https://example.com/blocklist.txt",
];

// Allowlist URLs (override blocklists)
const ALLOWLIST_URLS: [&str; 0] = [
    // "https://example.com/allowlist.txt",
];
```

## Development

### DevContainer

This project includes a devcontainer setup for VS Code / GitHub Codespaces:

```bash
# Start the dev container
docker compose -f .devcontainer/docker-compose.yaml up -d

# Enter the workspace
docker compose -f .devcontainer/docker-compose.yaml exec workspace bash
```

### Docker Build

```bash
docker build -t kube-dns-rs .
```

### Kubernetes Manifests

Deployment manifests are in the `kubernetes/` directory:

| File | Description |
|------|-------------|
| `deployment.yaml` | Kubernetes Deployment |
| `clusterrole.yaml` | RBAC ClusterRole for watching services |
| `clusterrolebinding.yaml` | RBAC ClusterRoleBinding |

## Project Structure

```
├── src/
│   ├── main.rs              # Entry point, server setup
│   ├── blocklist/
│   │   ├── domains.rs       # Blocklist fetching & trie management
│   │   ├── handler.rs       # Hickory ZoneHandler implementation
│   │   ├── trie.rs          # Trie data structure for domain matching
│   │   └── mod.rs
│   ├── kubernetes_svc/      # Kubernetes service discovery
│   │   ├── context.rs       # Service state context
│   │   ├── handler.rs       # Service zone handler
│   │   └── watcher.rs       # Kubernetes API watcher
│   └── kubernetes_crd/      # Kubernetes custom resources
│       ├── context.rs
│       ├── handler.rs
│       └── watcher.rs
├── kubernetes/              # K8s deployment manifests
├── .devcontainer/           # Dev container configuration
├── Dockerfile               # Multi-stage build
└── Cargo.toml
```

## License

Private / Proprietary
