# Stage 1: Build the fully static Rust application with Rustls
FROM rust:alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev ca-certificates

# Declare build arguments
ARG TARGETARCH

WORKDIR /app

# Conditionally install custom CA certificate
# This is done first to ensure cargo can access private crate registries.
COPY cacerts.pem .
RUN if [ -f "cacerts.pem" ]; then \
        cp cacerts.pem /usr/local/share/ca-certificates/cacerts.crt && \
        update-ca-certificates; \
    fi

# Set RUST_TARGET and install toolchain in one layer
RUN case ${TARGETARCH} in \
        "amd64") export RUST_TARGET="x86_64-unknown-linux-musl";; \
        "arm64") export RUST_TARGET="aarch64-unknown-linux-musl";; \
        *) echo "Unsupported architecture: ${TARGETARCH}" >&2; exit 1;; \
    esac && \
    rustup target add ${RUST_TARGET}

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    set -eux; \
    mkdir -p src; \
    echo "fn main() {}" > src/main.rs; \
    case ${TARGETARCH} in \
        "amd64") RUST_TARGET="x86_64-unknown-linux-musl";; \
        "arm64") RUST_TARGET="aarch64-unknown-linux-musl";; \
    esac; \
    export RUSTFLAGS='-C target-feature=+crt-static'; \
    cargo build --release --target ${RUST_TARGET}

# Build the application
COPY src ./src
RUN \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    set -eux; \
    case ${TARGETARCH} in \
        "amd64") RUST_TARGET="x86_64-unknown-linux-musl";; \
        "arm64") RUST_TARGET="aarch64-unknown-linux-musl";; \
    esac; \
    export RUSTFLAGS='-C target-feature=+crt-static'; \
    cargo build --release --target ${RUST_TARGET}

# --- Final Stages ---

# Intermediate stage for amd64
FROM alpine:latest AS release-amd64
WORKDIR /app
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/oracle-hcm-mcp .
COPY --from=builder /app/cacerts.pem .
RUN if [ -f "cacerts.pem" ]; then \
        apk --no-cache add ca-certificates && \
        mv cacerts.pem /usr/local/share/ca-certificates/cacerts.crt && \
        update-ca-certificates; \
    fi

# Intermediate stage for arm64
FROM alpine:latest AS release-arm64
WORKDIR /app
COPY --from=builder /app/target/aarch64-unknown-linux-musl/release/oracle-hcm-mcp .
COPY --from=builder /app/cacerts.pem .
RUN if [ -f "cacerts.pem" ]; then \
        apk --no-cache add ca-certificates && \
        mv cacerts.pem /usr/local/share/ca-certificates/cacerts.crt && \
        update-ca-certificates; \
    fi

# Final stage, selects the correct architecture
ARG TARGETARCH
FROM release-${TARGETARCH}

# Common metadata and settings
LABEL org.opencontainers.image.source="https://github.com/debanjanbasu/oracle-hcm-mcp"
LABEL org.opencontainers.image.description="Oracle HCM MCP"

EXPOSE 8080
CMD ["/app/oracle-hcm-mcp"]