# Security Policy

meter handles money-truth, so we take security seriously.

## Reporting a vulnerability

**Do not open a public issue for security vulnerabilities.** Instead, report privately via GitHub
Security Advisories ("Report a vulnerability" on the repository's Security tab), or email the
maintainer. Please include a description, reproduction steps, and impact. We aim to acknowledge within
a few business days and to coordinate a fix and disclosure timeline with you.

## Scope

Of particular interest: anything that can lose, double-count, or overspend credits; bypass HARD
limits or tenant isolation; or leak data across tenants. The ledger's invariants (no overdraft under
fault, exactly-once settlement, `enforced == billed`) are security-critical.

## Supported versions

meter is pre-1.0 and under active development; security fixes target the `main` branch. Versioned
release support will be documented here at the first tagged release.
