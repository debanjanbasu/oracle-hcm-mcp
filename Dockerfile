# Stage 1: Build the fully static Rust application with Rustls
FROM rust:alpine AS builder

# Set working directory early.
WORKDIR /app

# Conditionally install custom CA certificate for build-time.
# This must be the first step if building behind a corporate proxy.
COPY cacerts.pem .
RUN if [ -f "cacerts.pem" ]; then \
    cp cacerts.pem /usr/local/share/ca-certificates/cacerts.crt && \
    update-ca-certificates; \
    fi

# Install build dependencies for static linking.
RUN apk add --no-cache musl-dev

# Declare build arguments and set RUST_TARGET environment variable for subsequent commands.
ARG TARGETARCH
RUN case "${TARGETARCH}" in \
    "amd64") _rust_target="x86_64-unknown-linux-musl";; \
    "arm64") _rust_target="aarch64-unknown-linux-musl";; \
    *) echo "Unsupported architecture: ${TARGETARCH}" >&2; exit 1;; \
    esac && \
    echo "export RUST_TARGET=${_rust_target}" > /env.sh && \
    # Source the file to use RUST_TARGET in this layer for rustup
    . /env.sh && \
    rustup target add "${RUST_TARGET}"

# Set RUSTFLAGS for a static binary. This applies to all subsequent cargo commands.
ENV RUSTFLAGS='-C target-feature=+crt-static --cfg reqwest_unstable'

# Cache dependencies. This layer is rebuilt only when Cargo.toml or Cargo.lock change.
COPY Cargo.toml Cargo.lock ./
RUN \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    . /env.sh && \
    set -eux; \
    mkdir -p src; \
    echo "fn main() {}" > src/main.rs; \
    cargo build --release --target "$RUST_TARGET"

# Build the application.
COPY src ./src
RUN \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    . /env.sh && \
    set -eux; \
    cargo build --release --target "$RUST_TARGET" && \
    cp "target/$RUST_TARGET/release/oracle-hcm-mcp" /app/oracle-hcm-mcp

# --- Final Stage ---
# Use a scratch image for a minimal and secure runtime.
FROM scratch AS runtime

# Add metadata to the image.
LABEL org.opencontainers.image.source="https://github.com/debanjanbasu/oracle-hcm-mcp"
LABEL org.opencontainers.image.description="Oracle HCM MCP"

# Set a default logging level. This can be overridden by the .env file.
ENV RUST_LOG="info"

# Use a dedicated working directory. This is a best practice and can prevent
# issues with relative file paths in the application code.
WORKDIR /app

# Copy the application binary into the working directory.
COPY --from=builder /app/oracle-hcm-mcp .

# Expose the application port.
EXPOSE 8080

# Run the application from the working directory.
ENTRYPOINT ["/app/oracle-hcm-mcp"]
