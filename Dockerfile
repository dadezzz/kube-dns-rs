FROM git.zarantonello.dev/infra/ci-rust:v1.2.10@sha256:9a262fdf9deacad26388ed50255c66101d2caab8ee65248c2bb4d4433b1bf175 AS builder

WORKDIR /srv

COPY . .
RUN --mount=type=cache,sharing=locked,target=/usr/local/cargo/registry cargo build --release

# Final image.
FROM docker.io/library/alpine:3.24.3@sha256:be2656061b536c1883c4034790e21d0a09c5aaaf51968b89368a3c0371ea2c9a

COPY --from=builder /srv/target/release/kube-dns-rs /usr/local/bin/kube-dns-rs

ENTRYPOINT ["/usr/local/bin/kube-dns-rs"]
