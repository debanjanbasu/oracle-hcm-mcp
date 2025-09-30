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
