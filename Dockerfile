FROM rustlang/rust:nightly-slim AS builder

RUN apt-get update -y && \
    apt-get install -y pkg-config make g++ libssl-dev && \
    rustup target add x86_64-unknown-linux-gnu

WORKDIR /app
COPY . .

RUN cargo build --release --target x86_64-unknown-linux-gnu --all-features

# ------------------------------

FROM gcr.io/distroless/cc

WORKDIR /app

COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/psh-ohs /app/psh-ohs

ENTRYPOINT [ "/app/psh-ohs" ]