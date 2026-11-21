# hkferry

Hong Kong ferry routes, live ETAs and timetables as a Rust CLI and MCP server.

Covers Hong Kong & Kowloon Ferry (HKKF) and Sun Ferry via their public,
keyless open data.

## Install

**Agent skill** — installs `SKILL.md` and the binary bootstrap script:

```bash
npx skills add hongkong-ai-labs/hongkong-ferry-skill
```

**Prebuilt binary** — macOS (Intel or Apple silicon) and Linux (x86_64 or
aarch64, static musl), checksum-verified, into `~/.local/bin`:

```bash
curl -fsSL https://raw.githubusercontent.com/hongkong-ai-labs/hongkong-ferry-skill/main/scripts/install.sh | sh
```

**From source** — Rust 1.89 or newer:

```bash
git clone https://github.com/hongkong-ai-labs/hongkong-ferry-skill
cd hongkong-ferry-skill
cargo install --path . --locked
# or: cargo build --release --locked   # target/release/hkferry
```

Verify:

```console
$ hkferry --version
```

## CLI

```bash
hkferry routes
hkferry routes --operator hkkf
hkferry eta hkkf 2
hkferry eta hkkf 2 --direction inbound
hkferry eta sunferry CECC --json
hkferry timetable hkkf 1 --direction outbound
hkferry timetable sunferry CECC
hkferry mcp
```

## Routes

| Operator | Routes |
| --- | --- |
| HKKF | Central–Sok Kwu Wan, Central–Yung Shue Wan, Central–Peng Chau, Peng Chau–Hei Ling Chau |
| Sun Ferry | Central–Cheung Chau, Central–Mui Wo, North Point–Hung Hom, North Point–Kowloon City |

## MCP

```json
{
  "mcpServers": {
    "hongkong-ferry": { "command": "hkferry", "args": ["mcp"] }
  }
}
```

Tools: `list_ferry_routes`, `get_ferry_eta`, `get_ferry_timetable`.

## Limits

Live ETA is a prediction and can lag special arrangements. Star Ferry, Fortune
Ferry and the Discovery Bay / Park Island ferries are not included. Timetable
remarks are operator text; read the operator notice for a disruption.

## Run modes and deployment

| Mode | Start with | Use for |
| --- | --- | --- |
| CLI | `hkferry <command>` | Interactive use and shell scripts |
| JSON | `<command>` with `--json` | `jq`, automation, other programs |
| MCP stdio | `hkferry mcp` | Desktop and CLI MCP clients |
| Container | `docker run --rm -i hkferry mcp` | No local Rust toolchain |

### MCP and authentication

This server is **stdio-only**: the client launches the process and speaks MCP
over stdin/stdout. There is no listening port, so there is no network
authentication layer and no API-key setting. Optional API keys in this project
family apply only to servers that expose an HTTP transport. Keep the process
local and let the MCP client manage it.

Typical client entry:

```json
{
  "mcpServers": {
    "hongkong-ferry-skill": {
      "command": "hkferry",
      "args": ["mcp"]
    }
  }
}
```

If the client does not inherit `PATH`, use the absolute installed path.

### Docker and Apple container

```bash
docker build -t hkferry .
docker run --rm hkferry --help
docker run --rm -i hkferry mcp          # stdio MCP
docker compose run --rm stdio             # same, through Compose
```

Apple's `container` runtime uses the same OCI image:

```bash
container build -t hkferry .
container run --rm hkferry --help
container run --rm -i hkferry mcp
```

The runtime image contains the release binary and CA certificates, runs as a
non-root user, and defaults to `--help` so a bare `docker run` is harmless.

### Cloudflare and WASM

There is no Worker or WASM deployment. The binary talks directly to upstream
HTTP APIs over TLS and uses a Tokio runtime; packaging it as a stdio server is
the supported integration. A Worker would need a separate HTTP/JavaScript
adapter and is out of scope for this repo.

### Releases

Pushing a `v*` tag runs the release workflow, which checks the tag against
`Cargo.toml` and `CHANGELOG.md`, builds four targets, smoke-tests native
binaries and publishes:

- `hkferry-<tag>-<target>.tar.gz` for macOS and static Linux
- `SHA256SUMS` covering every tarball

`scripts/install.sh` downloads the latest release, verifies its checksum and
falls back to `cargo build --release --locked` when no release is available.
`just tag X.Y.Z` creates the annotated tag locally; `just release X.Y.Z` tags
and pushes to trigger the workflow.

## Development

```bash
just                 # fmt check, clippy with -D warnings, tests
just skill-check     # validate SKILL.md and shell syntax
just container       # build the image and print CLI help
cargo test --all --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

CI runs the same checks. Dependabot proposes weekly Cargo and GitHub Actions
updates, and the audit workflow checks the lockfile against the RustSec
advisory database.

## Licence

MIT. Ferry data remains subject to the operators' terms and DATA.GOV.HK terms.
