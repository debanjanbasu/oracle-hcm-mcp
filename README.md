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

1. (Optional) Westpac uses a custom internal certificate authority (CA) for SSL/TLS. If you're working within the Westpac network or need to interact with services that use Westpac's internal CA, you'll need to ensure that your development environment trusts this CA. You can do this by adding the Westpac CA certificate to your system's trusted certificate store.
   ```
      Put a file called cacerts.pem in the root of the project. This file should contain the Westpac CA certificate. The docker build process will copy this file into the container and update the CA certificates.
   ```
2. The project uses Docker for containerization. Ensure you have Docker installed on your machine. The image is built using multi-platform support to ensure compatibility across different architectures.
   ```
   docker buildx build --platform linux/amd64,linux/arm64 -t <your-dockerhub-username>/oracle-hcm-mcp:latest --push .
   ```
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