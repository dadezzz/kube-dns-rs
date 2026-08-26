FROM git.zarantonello.dev/infra/ci-rust:v1.2.0@sha256:58fbc899f02d3514a7607200abaf2a006f2a92147a5e6e1a0daeef88091cef5a AS builder

WORKDIR /srv

COPY . .
RUN --mount=type=cache,sharing=locked,target=/usr/local/cargo/registry cargo build --release

# Final image.
FROM docker.io/library/alpine:3.24.1@sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b

COPY --from=builder /srv/target/release/kube-dns-rs /usr/local/bin/kube-dns-rs

ENTRYPOINT ["/usr/local/bin/kube-dns-rs"]
