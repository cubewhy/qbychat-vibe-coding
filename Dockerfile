# Build stage
FROM rust:1.82 as builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
# Cache deps
RUN mkdir src && echo "fn main(){}" > src/main.rs && cargo build --release && rm -rf src
COPY . .
RUN cargo build --release

# Runtime stage
FROM gcr.io/distroless/cc-debian12
WORKDIR /app
COPY --from=builder /app/target/release/qbychat-vibe-coding /app/qbychat-vibe-coding
ENV RUST_LOG=info
EXPOSE 8080
USER 65532:65532
ENTRYPOINT ["/app/qbychat-vibe-coding"]
