# SideDNS

**Route any domain to any local service. Instantly. Without touching your system.**

[![Platform](https://img.shields.io/badge/Platform-Windows_%7C_macOS_%7C_Linux-blue)](#platform-support)
[![CI](https://github.com/Wilfried-Tech/sidedns/actions/workflows/ci.yml/badge.svg)](https://github.com/Wilfried-Tech/sidedns/actions/workflows/ci.yml)
[![Release](https://github.com/Wilfried-Tech/sidedns/actions/workflows/release.yml/badge.svg)](https://github.com/Wilfried-Tech/sidedns/actions/workflows/release.yml)
[![Crates.io](https://img.shields.io/crates/v/sidedns?logo=rust)](https://crates.io/crates/sidedns)
[![Crates.io Downloads](https://img.shields.io/crates/d/sidedns)](https://crates.io/crates/sidedns)
[![docs.rs](https://img.shields.io/docsrs/sidedns-core?logo=docs.rs&label=docs.rs)](https://docs.rs/sidedns-core)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

*Stop editing `/etc/hosts`. Stop hardcoding ports. Stop breaking your DNS when you close the terminal.*

---

## What is SideDNS?

SideDNS is a local DNS router and transparent HTTP/HTTPS proxy for developers.  
It lets you map any domain name to any local service — and undo it completely when you're done.

```bash
sidedns add api.myapp.com --port 3000
# → api.myapp.com now resolves to your local server, system-wide, instantly

sidedns daemon stop
# → everything reverts. no residue. your machine is exactly as you left it.
```

No permanent config changes. No manual cleanup. No broken DNS after a crash.

---

## Why SideDNS?

| Problem | Common workaround | SideDNS |
| ------- | ----------------- | ------- |
| Test a local API as `api.prod.com` | Edit `/etc/hosts` manually | `sidedns add api.prod.com --port 3000` |
| Multiple services with real domain names | Per-app proxy configs | One daemon, system-wide |
| HTTPS locally with a real certificate | Self-sign manually, fight the browser | `sidedns cert install --trust` + `--https` |
| Clean up when done | Remember to undo everything | `sidedns daemon stop` undoes everything |
| One command to launch + route | Two terminals, manual wiring | `sidedns run -d api.local fastapi dev` |

---

## Features

- **Transparent DNS routing** — rules apply system-wide, to every app on the machine
- **HTTP & HTTPS proxy** — TLS termination with on-demand signed certificates
- **WebSocket support** — upgrades are tunneled transparently
- **Ephemeral rules** — `sidedns run` creates a rule for the lifetime of a command, then removes it
- **Wildcard domains** — `*.myapp.local` matches all subdomains
- **Auto port detection** — `sidedns run` can detect which port your process opened
- **Daemon lifecycle** — background process, PID-managed, graceful shutdown
- **Watch mode** — `sidedns watch` streams rule changes in real time
- **Live event stream** — `sidedns watch` streams rule changes in real time
- **Safe by design** — warns before routing public domains, `sidedns clean` removes any residue
- **Cross-platform** — Windows, macOS, Linux

---

## Architecture

```text
┌───────────────────────────────────────────────────────┐
│                    sidedns daemon                     │
│                                                       │
│  ┌──────────────┐  ┌───────────────┐  ┌───────────┐   │
│  │  DNS Server  │  │  HTTP/HTTPS   │  │    IPC    │   │
│  │ 127.0.53.53  │  │  Proxy :80    │  │  Server   │   │
│  │    port 53   │  │  & :443       │  │           │   │
│  └──────┬───────┘  └──────┬────────┘  └─────┬─────┘   │
│         │                 │                 │         │
│         └─────────────────┴────────────────►│         │
│                                        SharedState    │
│                                        (RuleStore)    │
└───────────────────────────────────────────────────────┘
         ▲                                    ▲
         │ DNS queries                        │ IPC commands
    System DNS                          CLI / GUI
    (all apps)                    sidedns add / remove / ...
```

The daemon runs a DNS server, a reverse proxy, and an IPC server — all sharing the same rule store via a lock-free arc-swap structure. CLI and GUI communicate exclusively through IPC.

DNS returns the proxy IP (`127.0.0.42`), not the target service IP directly. This is necessary because DNS has no concept of ports — the proxy bridges the gap.

Rules are stored in a lock-free [arc-swap](https://crates.io/crates/arc-swap) structure. DNS and proxy reads never block, even during writes.

### Ephemeral vs persistent rules

| | Persistent | Ephemeral |
| ----- | --------- | --------- |
| Created by | `sidedns add` | `sidedns run` |
| Survives restart | Yes | No |
| Removed by | `sidedns remove` | Command exit / crash |
| Priority | Normal | Higher (shadows persistent rules) |

---

## Installation

### cargo install

```bash
cargo install sidedns
```

### Pre-built binaries

Download the latest binary for your platform from the [Releases](https://github.com/Wilfried-Tech/sidedns/releases) page.

| Platform | File |
| -------- | ---- |
| Linux x86_64 (glibc) | `sidedns-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` |
| Linux x86_64 (musl) | `sidedns-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 | `sidedns-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Intel | `sidedns-vX.Y.Z-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `sidedns-vX.Y.Z-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `sidedns-vX.Y.Z-x86_64-pc-windows-msvc.zip` |

Each archive includes a `.sha256` checksum file.

### From source

```bash
git clone https://github.com/Wilfried-Tech/sidedns
cd sidedns
cargo build --release
```

The `sidedns` binary is in `target/release/`.

### Platform requirements

| OS | Required |
| -- | -------- |
| Linux | `systemd-resolved` or compatible |
| macOS | macOS 12+ |
| Windows | Windows 10+ (PowerShell, admin for DNS config) |

---

## Quick Start

```bash
# 1. Start the daemon (background by default)
sidedns daemon start

# 2. Add a rule
sidedns add api.local --port 3000

# 3. Your service at localhost:3000 is now reachable at http://api.local
curl http://api.local/health

# 4. Done for the day
sidedns daemon stop
```

---

## Command Reference

### `sidedns daemon`

Manage the background daemon process.

```bash
sidedns daemon start            # start in background (default)
sidedns daemon start --no-background   # run in foreground (for debugging)
sidedns daemon stop             # graceful shutdown + revert DNS config
```

### `sidedns add`

Add or replace a DNS routing rule.

```bash
sidedns add <domain> [options]

Options:
  -i, --ip <IP>      Target IP address  [default: 127.0.0.1]
  -p, --port <PORT>  Target port        [default: 80]
      --https        Enable HTTPS TLS proxy for this rule
```

```bash
# Basic HTTP rule
sidedns add api.local --port 3000

# With a specific IP
sidedns add db.internal --ip 192.168.1.10 --port 5432

# HTTPS with TLS termination (requires cert install)
sidedns add secure.local --port 4000 --https

# Wildcard — matches all subdomains
sidedns add "*.myapp.local" --port 8080
```

### `sidedns remove`

Remove a routing rule.

```bash
sidedns remove api.local
```

### `sidedns list`

List all active routing rules (persistent and ephemeral).

```bash
sidedns list
```

```text
DOMAIN                    IP                 PORT          SECURE
api.local                 127.0.0.1          3000          no
secure.local              127.0.0.1          4000          yes
*.myapp.local             127.0.0.1          8080          no
```

### `sidedns resolve`

Resolve a domain to its configured target.

```bash
sidedns resolve api.local
# → api.local → 127.0.0.1:3000
```

### `sidedns run`

Run a command with an ephemeral DNS rule active for its lifetime.  
The rule is created before the command starts and removed when it exits — even on crash.

```bash
sidedns run -d <domain> [--ip IP] [--port PORT] [--https] -- <command> [args...]
```

```bash
# Auto-detect port after launch
sidedns run -d api.local -- npm run dev

# Explicit port
sidedns run -d api.local --port 3000 -- cargo run

# HTTPS proxy
sidedns run -d secure.local --port 4000 --https -- python -m uvicorn app:app

# Works with any command
sidedns run -d backend.test --port 8080 -- ./my-server
```

**Port auto-detection**: if `--port` is omitted, SideDNS waits for your process to open a port and configures the rule automatically. If multiple ports are opened, it prompts you to choose.

### `sidedns status`

Show whether the daemon is running and how many rules are active.

```bash
sidedns status
# daemon: running
# rules:  3
```

### `sidedns watch`

Stream rule changes to stdout in real time. Useful for scripting or monitoring.

```bash
sidedns watch
```

### `sidedns cert`

Manage the root CA certificate used for HTTPS proxying.

```bash
# Generate the CA (if not already done) and install it
sidedns cert install

# Generate + install + trust in all stores (requires admin)
sidedns cert install --trust

# Trust in specific stores
sidedns cert trust --system
sidedns cert trust --nss        # Firefox + Chrome on Linux
sidedns cert trust --java       # Java keystore via keytool

# Remove from all trust stores
sidedns cert untrust

# Untrust in specific stores
sidedns cert untrust [--system | --nss | --java]

# Uninstall the CA files entirely
sidedns cert uninstall
```

### `sidedns clean`

Remove any residual DNS configuration that may have been left behind by a previous crash or incomplete shutdown.

```bash
sidedns clean
```

Run this if DNS resolution seems broken after an unexpected daemon termination.

---

## HTTPS Support

SideDNS can terminate TLS for your local services, giving them a valid HTTPS certificate that browsers trust.

### How it works

1. SideDNS generates a local root CA and stores it in your data directory
2. You install and trust it once with `sidedns cert install --trust`
3. For each rule with `--https`, SideDNS signs a certificate on demand using that CA
4. The HTTPS proxy listens on `:443`, terminates TLS, and forwards plain HTTP to your service
5. Browsers see a valid certificate — no warnings

### Setup

```bash
# Requires admin/sudo
sudo sidedns cert install --trust
```

Firefox users: Firefox uses its own certificate store, independent of the system.

```bash
# Trust in Firefox/Chrome (Linux, via certutil)
sudo sidedns cert trust --nss
```

Firefox manual trust (all platforms):  
`Settings → Privacy & Security → View Certificates → Authorities → Import → select the CA file`  
The CA file is at the path shown by `sidedns cert install`.

### Supported trust stores

| Store | Tool used | Platforms |
| ----- | --------- | --------- |
| System | `security` / `certutil` / distro tools | macOS, Windows, Linux |
| NSS (Firefox, Chrome) | `certutil` (libnss3-tools) | Linux, macOS |
| Java | `keytool` | All (requires JDK) |

---

## Domain Safety

SideDNS accepts any valid domain name — there are no hard restrictions.

This is intentional: developers sometimes need to shadow real domains for integration testing, service mocking, or infrastructure simulation.

**However**, routing a real public domain (like `api.stripe.com`) through SideDNS means **every application on your machine** — not just your browser — will resolve that domain to your local service while the daemon is running. This includes CLI tools, package managers, background services, and anything else making network calls.

SideDNS surfaces a clear warning when you add a rule for a domain that appears to be a real public domain, and asks for explicit confirmation. For domains that are clearly local (`.local`, `.test`, `.internal`, `.localhost`, `.example`), no confirmation is required.

If the daemon stops unexpectedly, run `sidedns clean` to ensure no stale DNS configuration remains.

---

## Platform Support

### DNS configuration strategy

SideDNS uses **split DNS** — it routes only the domains you configure through its local resolver, leaving all other DNS traffic untouched. This makes it compatible with VPNs and other DNS tools running simultaneously.

| Platform | Mechanism | Admin required |
| ----- | --------- | --------- |
| Linux | `systemd-resolved` drop-in config | Yes (for DNS config) |
| macOS | `/etc/resolver/<domain>` files | Yes (for DNS config) |
| Windows | NRPT (Name Resolution Policy Table) | Yes (for DNS config) |

The daemon itself runs as a regular user process. Only the DNS system configuration step requires elevated privileges, performed at `daemon start` and reverted at `daemon stop`.

### VPN compatibility

Because SideDNS uses split DNS (not global DNS replacement), it is compatible with most VPN setups. Your VPN's DNS configuration handles its own namespaces; SideDNS only intercepts the domains you explicitly configure.

---

## How It Works Internally

### DNS resolution flow

```plain
Application makes DNS query for "api.local"
    ↓
System DNS → split DNS routes "api.local" to SideDNS (127.0.53.53:53)
    ↓
SideDNS DNS server: rule found → returns 127.0.0.42 (proxy address)
SideDNS DNS server: no rule → forwards to upstream DNS (your real resolver)
    ↓
Application connects to 127.0.0.42:80 or :443
    ↓
SideDNS proxy: reads Host header → looks up rule → forwards to target ip:port
```

### Rule store

Rules are stored in a lock-free arc-swap structure. Reads (DNS server, HTTP proxy) are non-blocking and never contend with writes. Writes (IPC add/remove) clone and atomically swap the rule set.

Persistent rules survive daemon restarts (stored via `confy`). Ephemeral rules (`sidedns run`) exist only in memory.

---

## Contributing

PRs and issues welcome. The codebase is structured as a Cargo workspace:

```text
sidedns/
├── core/       # daemon logic: DNS, proxy, IPC, certs, rule store
└── cli/        # CLI binary + command handlers
```

Run tests:

```bash
cargo test -p sidedns-core
cargo test -p sidedns-cli
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

Issues and PRs are welcome. Please open an issue before starting work on a significant feature.

---

## License

MIT — see [LICENSE](LICENSE).
