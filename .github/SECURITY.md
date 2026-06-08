# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| Latest  | ✅        |
| Older   | ❌        |

## Reporting a vulnerability

**Do not open a public issue for security vulnerabilities.**

Please use [GitHub Private Security Advisories](https://github.com/Wilfried-Tech/sidedns/security/advisories/new) to report a vulnerability privately.

Include:

- A description of the vulnerability and its potential impact
- Steps to reproduce
- Affected versions
- Any suggested fix if you have one

You will receive a response within 72 hours. If the issue is confirmed, a fix will be prioritized and released as soon as possible. You will be credited in the release notes unless you prefer to remain anonymous.

## Security model

SideDNS installs a local root CA into system trust stores when HTTPS support is enabled. The private key (`SideDNS-CA.key`) is stored in your local data directory and never leaves the machine. Any vulnerability that could expose this key or allow unauthorized cert signing should be reported immediately.
