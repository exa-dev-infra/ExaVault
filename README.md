<p align="center">
  <img src="public/logo.svg" alt="ExaVault Logo" width="380" />
</p>

<p align="center">
  <strong>Lightweight, self-hosted secret &amp; .env management engine for high-velocity developer teams.</strong><br>
  Built with 100% Pure Rust &bull; AES-256-GCM &bull; Model Context Protocol (MCP) &bull; Askama &bull; PostgreSQL
</p>

---

## 🚀 Key Features

- **100% Pure Rust Full-Stack**:
  - Embedded server-side rendering with **Askama** + modern dark-mode Tailwind CSS & Alpine.js.
  - Zero Node.js runtime, zero npm, zero `node_modules` overhead.
- **Enterprise-Grade Cryptography & Security**:
  - **AES-256-GCM Encryption at Rest**: 32-byte master key (`EXAVAULT_MASTER_KEY`) and fresh 12-byte IV per secret write/update. Database stores only ciphertext, IV, and auth tag.
  - **Machine / Service Tokens**: Format `exv_<32_random_hex>`. Displayed raw **only once** upon creation. Database stores strictly `SHA-256` hash (`token_hash`) and prefix (`exv_xxxx...`).
  - **Admin Dashboard Authentication**: Password hash / argon2 verification and signed HS256 JWT session tokens.
- **Relational Integrity with Cascading Deletes**:
  - PostgreSQL database (`projects`, `environments`, `secrets`, `service_tokens`) with `ON DELETE CASCADE`.
  - Automatic environment seeding (`dev`, `staging`, `prod`) whenever a project is created.
- **Web Dashboard Workflows**:
  - **Key-Value Table Editor**: Add/edit/delete rows, mask/unmask values (`••••••••`), copy to clipboard, and detect unsaved changes.
  - **Raw `.env` Bulk Mode**: Monospace text editor with bidirectional sync to the table, preserving `# comments` and multiline variables.
  - **Service Token Generator Modal**: Generate scoped environment tokens with configurable expiry (7d, 30d, 90d, 1y, Never) and copyable integration snippets.
- **Multi-Device Mobility**:
  - **CI/CD Export API (`GET /api/v1/export`)**: Supports `dotenv`, `shell`, and `json` formats with proper escaping.
  - **Built-in CLI (`exavault pull`)**: Single terminal command to download `.env` straight to current directory.
  - **Built-in MCP Server (`exavault mcp`)**: Standard JSON-RPC stdio protocol for AI IDEs.

---

## 🛠️ Quickstart

### 1. Environment Setup
Copy the example environment configuration:
```bash
cp .env.example .env
```
Ensure PostgreSQL is running (default `DATABASE_URL=postgres://postgres:postgres@localhost:54321/exavault?sslmode=disable`).

### 2. Run Database Migrations & Start Server
```bash
# Build & run web dashboard
cargo run -- serve
```
Open `http://localhost:3000` in your browser.  
Default Admin Credentials: `admin` / `adminsecret123`.

---

## 💻 CLI & Integration Usage

### 1. Direct File Download (`curl`)
```bash
curl -fsSL -H "Authorization: Bearer exv_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" http://localhost:3000/api/v1/export > .env
```

### 2. Process Memory Injection (`shell` format)
```bash
export $(curl -fsSL -H "Authorization: Bearer exv_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" "http://localhost:3000/api/v1/export?format=shell" | xargs)
```

### 3. ExaVault CLI Command
```bash
# Download to .env in current folder
exavault pull --token exv_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx --url http://localhost:3000 --out .env
```

### 4. Model Context Protocol (MCP) for AI Assistants
Add to your `~/.gemini/antigravity/mcp_config.json`, Cursor, or Claude Desktop:
```json
{
  "mcpServers": {
    "exavault": {
      "command": "/path/to/exavault",
      "args": ["mcp"],
      "env": {
        "EXAVAULT_URL": "http://localhost:3000",
        "EXAVAULT_TOKEN": "exv_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
      }
    }
  }
}
```
Now in your AI assistant, simply ask:
> *"Pull the latest dev .env for project Cyber Mobile"*

---

## 🧪 Testing

```bash
# Run ExaVault integration test suite
cargo test --test vault_test

# Run unit tests
cargo test --lib
```

---

## 📜 License
MIT License.
