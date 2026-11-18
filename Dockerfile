FROM git.zarantonello.dev/infra/ci-rust:v2026.11.18.1@sha256:71f421f52d38aaae8b7d2fdd61a457fb97a1d350ddaffcd3071fd0b901bfa59f AS builder

WORKDIR /srv

COPY . .
ENV RUSTFLAGS="-C target-cpu=x86-64-v3"
RUN --mount=type=cache,sharing=locked,target=/usr/local/cargo/registry cargo build --release

# Final image.
FROM scratch

COPY --from=builder /srv/target/release/kube-dns-rs /usr/local/bin/kube-dns-rs

ENTRYPOINT ["/usr/local/bin/kube-dns-rs"]
