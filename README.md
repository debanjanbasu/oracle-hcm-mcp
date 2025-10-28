Local development environment setup:

1. Clone the repository:

   ```
   git clone https://github.com/your-username/oracle-hcm-mcp.git
   ```

2. Install dependencies:

   ```
   cd oracle-hcm-mcp
   cargo build
   ```

3. Set up environment variables:

   ```
   cp .env.example .env
   ```

   This project uses dotenvx to load environment variables from the .env file at runtime. Ensure .env exists (you can copy from .env.example as shown). The application calls dotenvx on startup to load these values.

4. Run the application:
   ```
   dotenvx run -- cargo run
   ```

Building the project for production:

1. The project uses Docker for containerization. Ensure you have Docker installed on your machine. The image is built using multi-platform support to ensure compatibility across different architectures. If there's custom CA certificates needed, add it to the root build folder as `cacerts.pem`.
   ```
   docker buildx build --platform linux/amd64,linux/arm64 -t <your-dockerhub-username>/oracle-hcm-mcp:latest --no-cache --push .
   ```
2. (Optional) If you need to interact with services that use a custom internal certificate authority (CA), you can inject your custom CA certificate at runtime.
   **Note:** The final Docker image is based on `scratch` and does not include any system CA certificates. Therefore, custom certificates must be explicitly provided.
   To do this:
   - Ensure you have your custom CA certificate in a `.pem` file (e.g., `cacerts.pem` or `custom_cacerts.pem`).
     - **Verify File Content:** Double-check that the host file (`cacerts.pem`) is not empty and contains the expected certificate data.
     - **Check Permissions:** Ensure the file has appropriate read permissions for the Docker daemon (e.g., `chmod 444 cacerts.pem`).
   - **Important:** When using the `-v` flag to mount a certificate file, the source path (e.g., `/path/to/your/cacerts.pem`) must be an *absolute path* to an existing file on your host machine, or a *relative path* that correctly resolves to an existing file from your current working directory. If the specified source file does not exist, Docker may create an empty directory at that location inside the container instead of mounting your certificate. If the file exists but is empty on the host, it will be mounted as an empty file in the container.
   - When running the Docker container, mount this file into the container and set the `SSL_CERT_FILE` environment variable to its path within the container. For example, if your `cacerts.pem` is in your current directory:
     ```bash
     docker run \
       --name oracle-hcm-mcp \
       -p 8080:8080 \
       --env-file .env \
       -e RUST_LOG="trace" \
       -v "$(pwd)/cacerts.pem:/app/cacerts.pem" \
       -e SSL_CERT_FILE="/app/cacerts.pem" \
       <your-dockerhub-username>/oracle-hcm-mcp:latest
     ```
     ```powershell
     docker run --name oracle-hcm-mcp -p 8080:8080 --env-file .env -e 'RUST_LOG=trace' --mount "type=bind,source=$($PWD.Path)\cacerts.pem,target=/app/cacerts.pem,readonly" -e 'SSL_CERT_FILE=/app/cacerts.pem' debanjanbasu/oracle-hcm-mcp:latest
     ```
     Using `$(pwd)/cacerts.pem` ensures an absolute path is provided, preventing issues with the current working directory. This will make your custom CA certificate available to the application for SSL/TLS connections.
3. Run the Docker container (trace mode turned on for verbose logging):
   ```
   docker run --name oracle-hcm-mcp-debug -p 8080:8080 --env-file .env -e RUST_LOG="trace" debanjanbasu/oracle-hcm-mcp:latest
   ```

Tracing and Logging:
The application uses the tracing crate for structured logging. You can configure the log level using the RUST_LOG environment variable. For example, to set the log level to debug, you can run:
```
export RUST_LOG="debug"
OR
export RUST_LOG="trace"
```

For tracing the requests being made from the application, you can use:
```
RUST_LOG=reqwest=trace,reqwest_middleware=trace cargo run
```
This will provide detailed logs of the HTTP requests made by the reqwest client, however not log the payloads, as they can contain sensitive information (PII data).