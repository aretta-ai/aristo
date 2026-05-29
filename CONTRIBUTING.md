# Contributing to Aristo

Thanks for being here. Aristo is small, focused, and early — alpha,
with the architecture still settling. The most valuable contributions
right now are friction reports from real use, bugs with a
reproduction, and sharp questions about whether the model fits your
codebase. Code is welcome too; here's how to make it land cleanly.

## Where things go

- **Questions, ideas, feedback** → [Discussions](https://github.com/aretta-ai/aristo/discussions). This is what we want most right now.
- **Bugs, feature requests** → [Issues](https://github.com/aretta-ai/aristo/issues).
- **Security issues** → [`SECURITY.md`](./SECURITY.md) — please don't open a public issue.

## Before you write code

Aristo dogfoods itself and holds a strict working agreement — read
[`CLAUDE.md`](./CLAUDE.md) first. The short version:

- **Conventional commits** (`feat:`, `fix:`, `docs:`, …), one logical change each.
- **A CHANGELOG bullet in the same commit** as the change.
- **Tests before claims of correctness** — and the full suite (`fmt`, `clippy`, `test`) green before you commit.
- **Specifications are the truth** — we fix the implementation to match the spec, never the other way around.
- **Annotate as you go** — Aristo is its own first user.

## The loop

1. Open (or comment on) an issue so we can agree on the shape before you build.
2. Fork, branch, build — `cargo test --workspace` should be green.
3. Open a PR. CI gates it (the `aristo` annotation pipeline + standard Rust gates); both must pass.
4. We review. Small team, alpha — we respond as fast as we can; thanks for your patience.

By contributing, you agree your work is licensed under the project's [MIT license](./LICENSE), and you'll follow our [Code of Conduct](./CODE_OF_CONDUCT.md).
