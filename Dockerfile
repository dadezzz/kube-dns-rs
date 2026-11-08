FROM git.zarantonello.dev/infra/ci-rust:v2026.11.07.1@sha256:315a302f8dfcbb3afa63bfa73f895774d7e0321f4057332844afe19fad56cc21 AS builder

WORKDIR /srv

COPY . .
RUN --mount=type=cache,sharing=locked,target=/usr/local/cargo/registry cargo build --release

# Final image.
FROM scratch

COPY --from=builder /srv/target/release/kube-dns-rs /usr/local/bin/kube-dns-rs

ENTRYPOINT ["/usr/local/bin/kube-dns-rs"]
