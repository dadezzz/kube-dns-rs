FROM git.zarantonello.dev/infra/ci-rust:v2026.11.05.1@sha256:58c6ee29a1aa8cc7091296fd78adccdc50aa60901a2595a9ebd0897e519b1252 AS builder

WORKDIR /srv

COPY . .
RUN --mount=type=cache,sharing=locked,target=/usr/local/cargo/registry cargo build --release

# Final image.
FROM scratch

COPY --from=builder /srv/target/release/kube-dns-rs /usr/local/bin/kube-dns-rs

ENTRYPOINT ["/usr/local/bin/kube-dns-rs"]
