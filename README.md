# Quickdrop

A minimal file transfer service written in Rust.

This service allows for simple, secure, and ephemeral file sharing. It is built to be extremely lightweight, using a custom highly-optimized Rust implementation that streams data directly to disk with minimal memory overhead.

## Built With

*   [Rust](https://www.rust-lang.org/) - Systems programming language.
*   [Tokio](https://github.com/tokio-rs/tokio) - Asynchronous runtime for Rust.
*   [Hyper](https://github.com/hyperium/hyper) - Fast and correct HTTP implementation.

## Features

*   **High Performance**: Leveraging `hyper` and `tokio` for non-blocking I/O, capable of handling thousands of concurrent connections.
*   **Zero-Copy Streaming**: Files are streamed directly from the request body to the filesystem, keeping memory usage constant regardless of file size.
*   **Minimal Footprint**: The final Docker image is based on Alpine Linux and contains only the statically linked binary (approx. ~10MB).
*   **Ephemeral Storage**: Built-in retention policy automatically purges files older than a configured limit (default: 60 minutes).
*   **Secure**:
    *   **Path Traversal Protection**: Prevents malicious filenames from escaping the upload directory.
    *   **Security Headers**: Sends `X-Content-Type-Options`, `X-Frame-Options`, and `Strict-Transport-Security`.
    *   **Authentication**: Optional API Key header (`x-key`) for restricting uploads.
    *   **Limits**: Configurable file size limits and allowed file extensions.

## Usage

### Docker (Recommended)

Run the pre-built image from GitHub Container Registry:

```bash
docker run -d -p 3000:3000 ghcr.io/eltifi/quickdrop:latest
```

Or build locally:

```bash
docker build -t quickdrop .
docker run -p 3000:3000 quickdrop
```

### Local Development

1.  **Prerequisites**: Ensure you have [Rust](https://rustup.rs/) installed.
2.  **Run**:
    ```bash
    cargo run --release
    ```

## Configuration

The service is configured via environment variables. These can be set in a `.env` file or passed to the Docker container.

| Variable | Description | Default |
| :--- | :--- | :--- |
| `PORT` | The port the server listens on. | `3000` |
| `UPLOAD_DIR` | Directory where files are stored. | `uploads` |
| `KEY` | (Optional) Secret key required for uploads. | *None* (if set, `x-key` header is required) |
| `ID_LENGTH` | Length of the generated random file ID. | `5` |
| `MAX_FILE_SIZE` | Maximum allowed file size in bytes. | `1048576` (1MB) |
| `ALLOWED_FILE_TYPES`| Comma-separated list of allowed extensions (e.g. `.txt,.jpg`). | *All types allowed if unset* |
| `RETENTION_MINUTES` | Age in minutes after which files are auto-deleted. | `60` |

> **Production Use**: It is **strongly recommended** to set a `KEY` secret to prevent unauthorized access and abuse. Without it, anyone can upload files to your server.

> **Note**: The Docker image comes with default values baked in (including `KEY=your-secret-key`). Override them at runtime as needed:
> ```bash
> docker run -e KEY=my-secure-key -e RETENTION_MINUTES=120 ...
> ```

## API Documentation

### 1. Upload File

Upload a file to the server. If `KEY` is configured, you must provide the `x-key` header.

*   **Endpoint**: `PUT /` or `PUT /:filename`
*   **Headers**:
    *   `x-key`: (Required if configured) Your secret key.
    *   `Content-Length`: (Required) Size of the file.
*   **Body**: Raw file content.

**Example**:
```bash
# Upload a file named 'notes.txt'
curl --upload-file ./notes.txt -H "x-key: your-secret-key" http://localhost:3000/

# Response: http://localhost:3000/AbCdE
```

### 2. Download File

Download a file using its ID.

*   **Endpoint**: `GET /:id`
*   **Response**: The raw file content with `Content-Disposition: attachment`.

**Example**:
```bash
curl -O -J http://localhost:3000/AbCdE
```

### 3. Health Check

Check if the service is running.

*   **Endpoint**: `GET /health`
*   **Response**: `200 OK`

## Project Structure

This project uses a minimal Rust structure to reduce boilerplate:

*   `main.rs`: Contains the entire application logic (Server, Configuration, Handlers, Background Task).
*   `Dockerfile`: Multi-stage Alpine build definition.
*   `tests/`: Integration tests (Shell scripts) to verify functionality.

## License

MIT
