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
   docker buildx build --platform linux/amd64,linux/arm64 -t oracle-hcm-mcp:latest --push .
   ```
3. Run the Docker container:
   ```
   docker run -d -p 8080:8080 --env-file .env oracle-hcm-mcp:latest
   ```