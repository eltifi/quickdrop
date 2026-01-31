# Builder Stage
FROM rust:alpine AS builder

WORKDIR /app

# Install build dependencies for Alpine (musl-dev required)
RUN apk add --no-cache musl-dev

# Copy manifest
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to build dependencies
RUN echo "fn main() {}" > main.rs

# Build release dependencies (this layer is cached)
RUN cargo build --release

# Clean up dummy build artifacts to force rebuild of main.rs
RUN rm main.rs target/release/deps/quickdrop*

# Copy actual source
COPY main.rs ./

# Build release binary
RUN cargo build --release

# Runtime Stage
FROM alpine:latest

WORKDIR /app

# Create upload directory (optional, app creates it too)
RUN mkdir -p uploads

# Copy binary from builder
COPY --from=builder /app/target/release/quickdrop /app/quickdrop

# Build arguments
ARG PORT=3000

# Environment Defaults
ENV PORT=${PORT} \
    UPLOAD_DIR=uploads \
    ID_LENGTH=5 \
    MAX_FILE_SIZE=1048576 \
    ALLOWED_FILE_TYPES=.txt \
    RETENTION_MINUTES=60

# Expose port
EXPOSE ${PORT}

# Run
CMD ["/app/quickdrop"]
