
# Stage 1: Build the Rust application
FROM rust:latest AS builder

WORKDIR /app

# Copy Cargo.toml and Cargo.lock to leverage Docker cache
COPY Cargo.toml Cargo.lock ./

# Create a dummy src/main.rs and build dependencies.
# This step is crucial for Docker's build cache optimization.
# By building dependencies here, Docker can cache them. If only your application's
# source code changes, this layer remains valid, and subsequent builds will be faster
# as they won't need to recompile all dependencies.
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy the actual source code
COPY src ./src

# Build the application
RUN cargo build --release

# Stage 2: Create the final minimal image
FROM scratch

# Install any runtime dependencies if necessary (e.g., OpenSSL)
# For a basic Rust application, often no extra dependencies are needed
# If your application needs specific libraries, add them here.
# For example: RUN apt-get update && apt-get install -y libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/release/oracle-hcm-mcp ./oracle-hcm-mcp

# Expose the port your application listens on
EXPOSE 8080

# Run the application
CMD ["./oracle-hcm-mcp"]
