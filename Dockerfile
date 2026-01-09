# BlueVault - Blu-ray Archiving TUI Application
FROM rust:1.80-slim AS builder

# Install system dependencies needed for building
RUN apt-get update && apt-get install -y \
    pkg-config \
    libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Copy source code
COPY src/ src/

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    xorriso \
    growisofs \
    qrencode \
    rsync \
    sqlite3 \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -r -s /bin/false bluevault

# Create data directories
RUN mkdir -p /app/data /app/logs && \
    chown -R bluevault:bluevault /app

# Copy binary from builder
COPY --from=builder /app/target/release/bdarchive /usr/local/bin/bdarchive

# Set user
USER bluevault

# Set working directory
WORKDIR /app

# Set environment variables
ENV XDG_DATA_HOME=/app/data
ENV XDG_CONFIG_HOME=/app/data

# Volume for persistent data
VOLUME ["/app/data"]

# Default command
ENTRYPOINT ["bdarchive"]

# Labels for metadata
LABEL org.opencontainers.image.title="BlueVault" \
      org.opencontainers.image.description="Blu-ray archiving TUI application" \
      org.opencontainers.image.version="0.1.2" \
      org.opencontainers.image.authors="ChrisLAS" \
      org.opencontainers.image.source="https://github.com/ChrisLAS/blue-vault"
