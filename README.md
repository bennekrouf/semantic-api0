# Semantic

A tool that uses LLM to match user inputs with predefined API endpoints based on semantic understanding.


 cargo run -- --provider cohere --api http://0.0.0.0:50057

grpcurl -plaintext -d '{"email": "bennekrouf.mohamed@gmail.com"}' 0.0.0.0:50057 endpoint.EndpointService/GetApiGroups

## Prerequisites

- Rust (latest stable version)
- For Ollama mode: Ollama running locally with required models
- For Claude mode: Claude API key in `.env` file

## Installation

1. Clone and build:
```bash
git clone git@github.com:bennekrouf/semantic.git
cd semantic
cargo install --path .
```

2. Configure your environment:
   
   **For Ollama:**
   - Install Ollama from https://ollama.ai/
   - Ensure Ollama is running: `ollama serve`
   - Pull required models:
     ```bash
     ollama pull llama2
     ollama pull deepseek-r1:8b
     # or other models as specified in your config.yaml
     ```
   
   **For Claude:**
   - Get a Claude API key from https://www.anthropic.com/
   - Create a `.env` file in the project root:
     ```
     CLAUDE_API_KEY=your_api_key_here
     ```

3. For local testing generate certificates:
```bash
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes
```

## Usage

⚠️ **IMPORTANT**: Email is required ONLY when analyzing sentences in CLI mode. It is not needed when starting the gRPC server.

cargo run -- --provider cohere --api http://0.0.0.0:50057

### CLI Mode with Local Endpoints (email required)
```bash
# Analyze a sentence with Ollama
semantic --provider ollama --email user@example.com "schedule a meeting tomorrow at 2pm with John"

# Analyze a sentence with Claude (default provider)
semantic --email user@example.com "schedule a meeting tomorrow at 2pm with John"
```

### CLI Mode with Remote Endpoints (email required)
```bash
# Use a remote gRPC endpoint service for fetching endpoints
semantic --provider claude --api http://example.com:50053 --email user@example.com "schedule a meeting tomorrow at 2pm with John"
```

### gRPC Server Mode (email not required)
```bash
# Start the gRPC server with Ollama (using local endpoints)
semantic --provider ollama

# Start the gRPC server with Claude (default provider, using local endpoints)
semantic

# Start the gRPC server with remote endpoints
semantic --provider claude --api http://example.com:50053
```

### Help
```bash
# Show help
semantic --help
```

⚠️ **IMPORTANT**: The following arguments are required in specific contexts:
- `--provider` - Optional, defaults to 'claude'
  - `ollama` - Use local Ollama instance (default host: localhost:11434)
  - `claude` - Use Claude API (requires API key in .env file)
- `--email` - Required ONLY when analyzing a sentence in CLI mode

## Client Authentication

When connecting to the Semantic server with a gRPC client, you must provide an email in the metadata for each request:

```
email: user@example.com
```

Requests without a valid email will be rejected with an error.
