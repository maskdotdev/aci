# Security Policy

## Supported Versions

Security fixes are handled on the `main` branch until the project starts
maintaining separate release branches.

## Reporting a Vulnerability

Report vulnerabilities through GitHub Security Advisories for
`maskdotdev/aci`. Include the affected version or commit, reproduction steps,
and the expected impact.

If GitHub Security Advisories are unavailable, open a minimal public issue that
asks for a private contact path without disclosing exploit details.

## Dependency Checks

Run this before releases and after dependency updates:

```sh
cargo audit
```
