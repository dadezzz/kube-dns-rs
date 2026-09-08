# AGENTS.md

Guidance for AI coding agents working in this repository. Read this before
making changes.

## Project overview

`kube-dns-rs` is a Kubernetes-aware DNS server written in Rust. It runs as a
container inside a Kubernetes cluster and answers DNS queries from three
sources, in precedence order:

1. `DnsRecord` custom resources (arbitrary A/AAAA records) —
   `src/kubernetes/crd/`
2. Block/allow lists (blocked names → NXDOMAIN) — `src/blocker/`
3. Recursive resolution upstream over DoT (Quad9, Cloudflare) — `src/resolver/`

Plus one zone handler for the cluster domain that derives records from
Kubernetes `Service`/`EndpointSlice` objects — `src/kubernetes/svc/`.

Two binaries in `Cargo.toml`: `kube-dns-rs` (the server) and `crdgen`
(regenerates the `DnsRecord` CRD YAML from the Rust type).

## Architecture

The entry point is `src/bin/kube-dns-rs.rs`. The flow:

1. Parse CLI args (`src/args.rs`, `--config`, default
   `/etc/kube-dns-rs/config.yaml`).
2. Load YAML settings (`src/settings.rs`): blocker list URLs,
   `kubernetes.cluster_domain`, TCP/UDP listeners.
3. Build a kube client (`src/init.rs`) and a hickory `Catalog`.
4. Download block/allow lists **synchronously** before binding listeners (so
   blocking is active from the first query).
5. Register zone handlers: root zone gets `KubernetesCrdZoneHandler` →
   `BlockerZoneHandler` → `ResolverZoneHandler`; the cluster domain gets
   `KubernetesSvcZoneHandler`.
6. Bind UDP + TCP sockets (port 1053 by default), then run refresher/watcher
   tasks in a `tokio::select!` until `ctrl-c`, then shut down gracefully.

### Shared context pattern

Each feature keeps live state in a `*Context` struct (`#[derive(Default)]`,
plain `HashMap`/`Trie` fields), wrapped in `Arc<RwLock<...>>`, owned by its
handler and mutated by a watcher/refresher task (which the binary drives to
completion before/can run alongside the server):

- `BlockerContext` — `HashMap<url, Trie<ListType>>`, refreshed by
  `BlockerRefresher` every 48h.
- `KubernetesCrdContext` — `HashMap<k8s-ref, DnsRecord>` + a rebuilt
  `Trie<Vec<DnsRecordData>>` index; `KubernetesCrdWatcher` syncs it.
- `KubernetesSvcContext` — `HashMap`s of `ServiceEntry` and
  `EndpointSliceEntry`; `KubernetesSvcWatcher` runs two watcher tasks
  (Services + EndpointSlices).

Zone handlers implement hickory's `ZoneHandler` trait (see any `handler.rs`).
Shared record-set/aggregation helpers live in `src/utils/mod.rs`
(`new_a_record_set`, `new_aaaa_record_set`, `break_with_nxdomain`,
`continue_with_recordset`, `handler_search_aggregator`, `name_to_labels`).
Domain matching uses the generic prefix trie in `src/trie.rs`.

## Conventions & gotchas

- **Language**: Rust 2024 edition with the dependencies in `Cargo.toml`. crates
  named like `hickory-server`, `hickory-resolver`, `kube`, `tokio`, `schemars`
  are forks/vendored variants published by Zarantonello (see the Cargo.lock). Do
  not "standardize" imports or APIs to upstream Rust crates — the project
  intentionally uses these.
- **Config**: YAML via `config` crate; structs must derive `Deserialize` (see
  `src/settings.rs`). Keep `examples/config.yaml` and
  `examples/kubernetes/configmap.yaml` config-data in sync with any schema
  change.
- **CRDs**: the `DnsRecordCrd` type (`src/kubernetes/crd/mod.rs`) is the single
  source of truth; regenerate with
  `cargo run --bin crdgen > examples/crds/dnsrecord.yaml` after changing it.
- **Behavioral rules to preserve**:
  - Block lists are fetched synchronously at startup, before listeners bind.
  - Allow lists take priority over block lists (tie-break in
    `BlockerContext::lookup`).
  - Blocked names → `NXDOMAIN` (`break_with_nxdomain`); unknown CRD names →
    `Skip` (fall through to resolver); known-but-wrong-type → `Empty`.
  - Headless services resolve from ready, non-terminating EndpointSlice
    endpoints; task referencing a k8s name is resolved via
    `<endpoint>.<svc>.<ns>.svc`.
  - Refresher/watcher `run()` methods keep the process alive when there are no
    tasks (spawn a pending task) — don't remove that.
- **Lifecycle**: tasks live in `JoinSet`s and are aborted on `Drop`; the binary
  calls `server.shutdown_gracefully()` on ctrl-c.

## Testing

Run:

```sh
cargo build          # or cargo build --release
cargo run --bin crdgen   # regenerate CRD YAML
```

There are no Rust unit tests; validation happens through the CI checks in
`.forgejo/workflows/` (format, yamllint, prettier, commitizen, build) and by
running the server against a real cluster (see `examples/kubernetes` for RBAC
and deployment).

## CI / release

- Forgejo workflows in `.forgejo/workflows/`.
- Commits must be Conventional Commits (enforced by `check-commitizen` on PR
  titles).
- Releases: `semantic-release` + `forgejo-release` plugin, `.releaserc.json` on
  `main`; building/pushing the container image is triggered by `v*` tags
  (`build.yaml`).
- Dependency updates are automated (Renovate, `.renovaterc.jsonc`); routine
  dependency bumps typically don't need feature-adjacent changes.
