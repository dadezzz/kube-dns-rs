FROM git.zarantonello.dev/infra/ci-rust:v2026.10.30.3@sha256:f3d4d23cec81f28d1f239884a35e56bca1147ba1d874e01da543f32ee04bb24f AS builder

WORKDIR /srv

COPY . .
RUN --mount=type=cache,sharing=locked,target=/usr/local/cargo/registry cargo build --release

# Final image.
FROM scratch

COPY --from=builder /srv/target/release/kube-dns-rs /usr/local/bin/kube-dns-rs

ENTRYPOINT ["/usr/local/bin/kube-dns-rs"]
