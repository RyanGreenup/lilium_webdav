# Build stage
FROM rust:1.83-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to cache dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release && rm -rf src target/release/deps/webdav_server*

# Copy actual source code
COPY src ./src
COPY db ./db

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/webdav_server /usr/local/bin/webdav_server

# Copy schema for reference (optional, for initializing new databases)
COPY schema.sql /app/schema.sql

# Create data directory
RUN mkdir -p /data

# Expose the default WebDAV port
EXPOSE 4918

# Set default environment variables
ENV WEBDAV_HOST=0.0.0.0
ENV WEBDAV_PORT=4918
ENV WEBDAV_DATABASE=/data/notes.db
ENV WEBDAV_USERNAME=admin
ENV WEBDAV_PASSWORD=changeme

# Run the server
CMD webdav_server serve \
    --database "${WEBDAV_DATABASE}" \
    --host "${WEBDAV_HOST}" \
    --port "${WEBDAV_PORT}" \
    --username "${WEBDAV_USERNAME}" \
    --password "${WEBDAV_PASSWORD}" \
    ${WEBDAV_USER_ID:+--user-id "${WEBDAV_USER_ID}"}
