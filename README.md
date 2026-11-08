# kube-dns-rs

A Kubernetes-aware DNS server written in Rust. It exposes DNS records derived
from Kubernetes primitives (Services, EndpointSlices, and a custom `DnsRecord`
CRD), blocks domains using remote block/allow lists, and recursively resolves
everything else over DNS-over-TLS upstreams.

The server listens on TCP and UDP (port `1053` by default) and is designed to
run as a container inside the cluster it serves.

## Features

- **Kubernetes-native records** — watches `Services` and `EndpointSlices` in
  every namespace and serves `A`/`AAAA` records under your cluster domain.
- **Custom `DnsRecord` CRD** — declare arbitrary `A`/`AAAA` records for any
  fully qualified domain name (e.g. `example.com`) from inside Kubernetes.
- **Recursive resolver** — anything not handled by Kubernetes records or block
  lists is resolved upstream over TLS (Quad9 and Cloudflare by default).
- **Blocking** — wildcard block lists (with optional allow lists) are downloaded
  on startup and refreshed periodically; blocked names return `NXDOMAIN`.
- **Cluster domain SOA** — serves a valid `SOA` record for the configured
  cluster domain.

## How it works

The server registers three zone handlers on the root zone (plus one on the
cluster domain):

| Handler                    | Zone               | Responsibility                                                       |
| -------------------------- | ------------------ | -------------------------------------------------------------------- |
| `KubernetesCrdZoneHandler` | `.` (root)         | Serves `DnsRecord` CRD entries (exact FQDN matches, A/AAAA, TTL 30s) |
| `BlockerZoneHandler`       | `.` (root)         | Consults block/allow lists and returns `NXDOMAIN` for blocked names  |
| `ResolverZoneHandler`      | `.` (root)         | Forwards all remaining queries upstream over DNS-over-TLS            |
| `KubernetesSvcZoneHandler` | `<cluster_domain>` | Serves `Service`/`EndpointSlice` records                             |

Lookup order is determined by the catalog registered in
`src/bin/kube-dns-rs.rs`: CRD records take precedence, then blocking, then
recursive resolution. Queries for names under the cluster domain are answered by
the SVC handler.

### Service / EndpointSlice records

Services and EndpointSlices are watched cluster-wide and produce records in the
cluster domain with the scheme:

```
<service-name>.<namespace>.svc.<cluster-domain>
<endpoint-name>.<service-name>.<namespace>.svc.<cluster-domain>
```

- A service with `clusterIP` resolves to that address.
- Headless services (no `clusterIP`) resolve to the addresses of their ready,
  non-terminating EndpointSlice endpoints.
- Endpoint-slice targets that reference Kubernetes deployments are resolved by
  their `<endpoint>.<service>.<namespace>.svc` name; external targets provide
  addresses directly.
- Port information from EndpointSlices is also tracked (SRV support is planned —
  the pattern is sketched in `src/kubernetes/svc/handler.rs`).

### The `DnsRecord` CRD

```yaml
apiVersion: zarantonello.dev/v1
kind: DnsRecord
metadata:
  name: example-com
  namespace: your-namespace
spec:
  fqdn: example.com
  data:
    - A:
        addresses:
          - 192.0.2.1
    - AAAA:
        addresses:
          - "2001:db8::1"
```

The CRD is type-derived from the Rust struct `DnsRecordCrd` and can be
regenerated at any time:

```sh
cargo run --bin crdgen > dnsrecord.yaml
```

### Domain blocking

Block and allow lists are plain text files, one domain per line, `#` for
comments, `*.` prefixes for wildcard entries. Lists are downloaded synchronously
at startup (before the listeners bind, so protection is in place immediately)
and refreshed periodically with automatic retry on failure. Allow lists always
win over block lists on ties (see `BlockerContext::lookup` for the exact
priority rules).

## Getting started

### Build

```sh
cargo build --release
```

Binaries:

- `kube-dns-rs` — the DNS server
- `crdgen` — generates the `DnsRecord` CRD definition YAML

### Configure

`kube-dns-rs` loads its configuration from `/etc/kube-dns-rs/config.yaml` by
default (override with `--config <path>`):

```yaml
blocker:
  allowlist_urls:
    - https://badblock.celenity.dev/wildcards-star/push_whitelist.txt
  blocklist_urls:
    - https://gitlab.com/hagezi/mirror/-/raw/main/dns-blocklists/wildcard/dyndns.txt
    # ...
kubernetes:
  cluster_domain: k8s.zarantonello.dev
listeners:
  tcp: "[::]:1053"
  udp: "[::]:1053"
```

### Run locally

```sh
# Against a local/remote cluster (kube credentials via the default kube client lookup):
kube-dns-rs --config examples/config.yaml

# Point a resolver at it:
dig @127.0.0.1 -p 1053 my-service.my-namespace.svc.k8s.zarantonello.dev
dig @127.0.0.1 -p 1053 example.com
```

### Deploy to Kubernetes

Reference manifests live in [`examples/kubernetes/`](examples/kubernetes/):

- `namespace.yaml`, `serviceaccount.yaml`, `clusterrole.yaml`,
  `clusterrolebinding.yaml` — RBAC: the server needs `watch` on `services`,
  `discovery.k8s.io/endpointslices`, and `zarantonello.dev/dnsrecords`.
- `config.yaml`/`configmap.yaml` — the runtime configuration.
- `deployment.yaml` — container image, TCP/UDP port `1053`, read-only root FS,
  non-root user, and a startup probe.
- `service.yaml` — optional cluster `Service` exposing the DNS port and giving
  the server a stable `clusterIP` (which you can point your k8s DNS at).

Apply the CRD first:

```sh
kubectl apply -f examples/crds/dnsrecord.yaml
```

## Project layout

```
src/
├── bin/
│   ├── kube-dns-rs.rs       # main server binary: wiring, catalog, listeners, lifecycle
│   └── crdgen.rs            # generates the DnsRecord CRD YAML
├── args.rs                  # CLI argument parsing (--config)
├── settings.rs              # YAML config schema
├── init.rs                  # logger, settings load, socket binding, kube client
├── trie.rs                  # prefix trie used for domain matching
├── blocker/                 # block/allow list engine
│   ├── context.rs           # trie-backed store + priority lookup
│   ├── handler.rs           # zone handler returning NXDOMAIN for blocked names
│   └── refresher.rs         # initial download + periodic refresh tasks
├── kubernetes/
│   ├── crd/                 # DnsRecord CRD: context, zone handler, watcher
│   └── svc/                 # Service/EndpointSlice: context, zone handler, watcher
├── resolver/mod.rs          # recursive resolver zone handler (DoT upstreams)
└── utils/mod.rs             # shared record-set/aggregation helpers
```

### Development

The repository is formatted with the project's Rust formatter and linted via the
CI checks in `.forgejo/workflows/`. Commits must follow the
[Conventional Commits](https://conventionalcommits.org/) style (`feat:`, `fix:`,
`chore(deps):`, ...) as enforced by the commitizen check; releases are cut with
`semantic-release` and the `forgejo-release` plugin. When in doubt, check the CI
workflows before pushing.

## License

MIT
