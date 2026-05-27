# Filen-MCP

MCP server for [Filen](https://filen.io) cloud storage. Exposes 17 tools
to LLM clients (Claude Desktop, Cursor, etc.) over the Model Context Protocol.

## Features

- **Local-first** — file data never travels through the JSON-RPC channel.
  The LLM sends path strings; the binary reads/writes files locally.
- **SDK-driven** — uses the official `filen-sdk-rs` for all API interactions.
- **One-time auth** — interactive login stores encrypted credentials;
  subsequent server starts need no password.

## Quick Start

```sh
# Clone and build
git clone https://github.com/your-org/filen-mcp
cd filen-mcp
just dev

# Authenticate (interactive — email, password, optional 2FA)
cargo run -- login

# Start the MCP server over stdio
cargo run -- serve
```

The auth config is persisted to:
- **Windows**: `%APPDATA%\filen-mcp\auth.json`
- **Unix**: `$HOME/.config/filen-mcp/auth.json`

Override with the `FILEN_AUTH_CONFIG` env var, or pass JSON directly
via `FILEN_AUTH_CONFIG_JSON` for container/CI usage.

## MCP Tools

| Tool | Description |
|------|-------------|
| `filen_ls` | List directory contents |
| `filen_mkdir` | Create a directory |
| `filen_upload` | Upload a local file |
| `filen_download` | Download a remote file |
| `filen_delete` | Delete a file/directory |
| `filen_mv` | Move a file/directory |
| `filen_stat` | Get file/directory metadata |
| `filen_search` | Search by name |
| `filen_quota` | Get current storage quota info |
| `filen_whoami` | Get authenticated user info |
| `filen_notes_list` | List all notes |
| `filen_note_get` | Get a note by UUID |
| `filen_note_create` | Create a note |
| `filen_note_update` | Update a note |
| `filen_note_delete` | Delete a note |
| `filen_note_archive` | Archive a note |
| `filen_note_trash` | Trash a note |
| `filen_note_restore` | Restore a note from trash/archive |
| `filen_shares_in` | List items shared with you |
| `filen_shares_out` | List items you shared |
| `filen_ls_trash` | List trash contents |
| `filen_empty_trash` | Empty the Filen trash |

## MCP Client Configuration

Add to your MCP client config (e.g. Claude Desktop, Cursor):

```json
{
  "mcpServers": {
    "filen": {
      "command": "/path/to/filen-mcp",
      "args": ["serve"]
    }
  }
}
```

Run `filen-mcp login` first to create the auth config, then connect
via `filen-mcp serve`.

## Development

```sh
just         # fmt + lint + build
just dev     # fmt + lint + debug build
just release # fmt + lint + release build
just test    # fmt + lint + test
```

Requires Rust nightly (pinned via `rust-toolchain.toml`).

## License

[MIT](LICENSE)
