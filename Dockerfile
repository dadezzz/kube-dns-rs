FROM git.zarantonello.dev/infra/ci-rust:v2026.11.04.1@sha256:62c3284102ae8d3f3ea2749b897b1d29dc0ef0a5b5ad6671e357d5099319bb41 AS builder

WORKDIR /srv

COPY . .
RUN --mount=type=cache,sharing=locked,target=/usr/local/cargo/registry cargo build --release

# Final image.
FROM scratch

COPY --from=builder /srv/target/release/kube-dns-rs /usr/local/bin/kube-dns-rs

ENTRYPOINT ["/usr/local/bin/kube-dns-rs"]
