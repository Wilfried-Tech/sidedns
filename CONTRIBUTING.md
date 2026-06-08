# Contributing to SideDNS

Thanks for taking the time to contribute. This document covers everything you need to get started.

---

## Table of contents

- [Code of conduct](#code-of-conduct)
- [How to contribute](#how-to-contribute)
- [Development setup](#development-setup)
- [Project structure](#project-structure)
- [Coding standards](#coding-standards)
- [Commit convention](#commit-convention)
- [Running tests](#running-tests)
- [Submitting a pull request](#submitting-a-pull-request)

---

## Code of conduct

Be respectful. Constructive criticism is welcome. Personal attacks are not.

---

## How to contribute

**Bug reports** → open a [Bug Report](https://github.com/Wilfried-Tech/sidedns/issues/new?template=bug_report.yml) issue.

**Feature requests** → open a [Feature Request](https://github.com/Wilfried-Tech/sidedns/issues/new?template=feature_request.yml) issue first, before writing code. This avoids duplicate work and lets us align on design before implementation.

**Documentation fixes, typos** → PRs welcome without a prior issue.

**Security vulnerabilities** → see [SECURITY.md](.github/SECURITY.md). Do not open a public issue.

---

## Development setup

### Prerequisites

- Rust stable (2024 edition) — install via [rustup](https://rustup.rs)
- On Linux: `systemd-resolved` for DNS config tests
- On macOS: Xcode Command Line Tools
- On Windows: Visual Studio Build Tools (MSVC)

### Clone and build

```bash
git clone https://github.com/Wilfried-Tech/sidedns
cd sidedns
cargo build
```

### Run the daemon locally

```bash
cargo run -p sidedns-cli -- daemon start --no-background
```

---

## Project structure

```text
sidedns/
├── core/                   # Library crate — all daemon logic
│   ├── src/
│   │   ├── dns/            # DNS server + system DNS configurators
│   │   ├── proxy/          # HTTP/HTTPS reverse proxy
│   │   ├── ipc/            # IPC server, client, message types
│   │   ├── certs/          # CA generation, cert signing, trust stores
│   │   ├── store.rs        # Lock-free rule store (arc-swap)
│   │   ├── state.rs        # Shared daemon state
│   │   └── runner.rs       # Daemon entry point
│   └── tests/              # Integration tests
├── cli/                    # CLI frontend
│   └── src/
│       ├── commands/       # One file per CLI command
│       ├── cli.rs          # Clap definitions
│       └── lib.rs          # execute_from_command_line()
└── src/                    # Binary crate — CLI frontend / GUI (soon)
    └── main.rs         # Entry point + daemon detection
```

---

## Coding standards

These are enforced by CI. PRs that fail `clippy` or `fmt` will not be merged.

### Style

- **English only** — code, comments, doc comments
- **No inline comments** — if code needs explanation, refactor or write a doc comment
- **No `unwrap()` in non-test code** — use `?`, `expect()` with a meaningful message, or handle the error
- **Typed** — use specific types, avoid `Box<dyn Any>` and `impl Fn` where a named trait suffices
- **No dead code** — run `cargo check` before pushing

### Formatting

```bash
cargo fmt --all
```

### Lint

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Naming

- Modules: `snake_case`
- Types / traits: `PascalCase`
- Functions / methods: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Files: `snake_case.rs`

---

## Commit convention

SideDNS uses [Conventional Commits](https://www.conventionalcommits.org/). The changelog is generated automatically from commit messages — please follow the format.

```text
<type>(<scope>): <short description>

[optional body]

[optional footer: BREAKING CHANGE: ...]
```

### Types

| Type | When to use |
| ---- | ----------- |
| `feat` | New feature |
| `fix` | Bug fix |
| `perf` | Performance improvement |
| `refactor` | Code change that is neither a fix nor a feature |
| `docs` | Documentation only |
| `test` | Adding or fixing tests |
| `chore` | Dependency updates, config changes |
| `ci` | CI/CD workflow changes |
| `build` | Build system changes |

### Scope (optional)

`dns`, `proxy`, `ipc`, `certs`, `store`, `cli`, `daemon`, `ci`

### Examples

```text
feat(proxy): add WebSocket upgrade support
fix(dns): wildcard invalidation now clears matching subdomains
docs: update HTTPS setup instructions
chore: bump rcgen to 0.14
feat!: rename --ip flag to --target-ip (BREAKING CHANGE)
```

Breaking changes must include `!` after the type and a `BREAKING CHANGE:` footer.

---

## Running tests

```bash
# All tests
cargo test --workspace

# Core only (faster)
cargo test -p sidedns-core

# Specific test
cargo test -p sidedns-core store::tests::wildcard_matches_subdomain

# With output
cargo test -p sidedns-core -- --nocapture
```

Tests that require elevated privileges (DNS system config, CA install) are gated behind `#[ignore]`. Run them explicitly with:

```bash
sudo cargo test -p sidedns-core -- --ignored
```

---

## Submitting a pull request

1. Fork the repository and create a branch from `master`
2. Name your branch: `feat/my-feature`, `fix/my-bug`, `docs/my-change`
3. Make your changes following the coding standards above
4. Add or update tests where appropriate
5. Run `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings`
6. Push and open a PR against `master`
7. Fill in the [PR template](.github/PULL_REQUEST_TEMPLATE.md)

PRs targeting `master` directly with breaking changes should be discussed in an issue first.

All PRs are reviewed before merge. Expect feedback. We aim to respond within a few days.
