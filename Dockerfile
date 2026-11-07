FROM git.zarantonello.dev/infra/ci-rust:v2026.11.06.1@sha256:b6ee299b3759bd9ca63b067821a709dc573f311e5e90cfc375544cf245875356 AS builder

WORKDIR /srv

COPY . .
RUN --mount=type=cache,sharing=locked,target=/usr/local/cargo/registry cargo build --release

# Final image.
FROM scratch

COPY --from=builder /srv/target/release/kube-dns-rs /usr/local/bin/kube-dns-rs

ENTRYPOINT ["/usr/local/bin/kube-dns-rs"]
