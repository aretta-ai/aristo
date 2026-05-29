# Security policy

Aristo is alpha software — we still want to know about
vulnerabilities, and we'll treat them seriously.

## Reporting a vulnerability

Please report security issues **privately** — not in public issues or
Discussions:

- **Preferred:** open a [GitHub Security Advisory](https://github.com/aretta-ai/aristo/security/advisories/new) on this repo.
- **Or email:** security@aretta.ai

Include what we'd need to reproduce it: affected version or commit,
steps, and the impact you see.

## What to expect

- We acknowledge reports within **3 business days**.
- We'll work with you on a fix and coordinate disclosure — typically within **90 days**, sooner when we can.
- We're glad to credit you in the advisory unless you'd rather stay anonymous.

## Scope

**In scope:** the `aristo` CLI, the SDK crates (`aristo-core`,
`aristo-macros`, `aristo`), and the annotation/index format.

**Out of scope (for now):** Aretta's server-side backend and anything
behind `verify = "full"` — that isn't part of this OSS repo and has
its own channel.
