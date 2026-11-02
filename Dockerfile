FROM git.zarantonello.dev/infra/ci-rust:v1.2.11@sha256:686d8599b898d97ecf84c04741943ef5fefea5ad78dc49fb4753ee854b99aa9f AS builder

WORKDIR /srv

COPY . .
RUN --mount=type=cache,sharing=locked,target=/usr/local/cargo/registry cargo build --release

# Final image.
FROM scratch

COPY --from=builder /srv/target/release/kube-dns-rs /usr/local/bin/kube-dns-rs

ENTRYPOINT ["/usr/local/bin/kube-dns-rs"]
