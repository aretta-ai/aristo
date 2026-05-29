# Aristo

![aristo verified](./docs/badge.svg)
[![crate](https://img.shields.io/crates/v/aristo.svg)](https://crates.io/crates/aristo)
[![docs](https://docs.rs/aristo/badge.svg)](https://docs.rs/aristo)
[![license](https://img.shields.io/crates/l/aristo.svg)](./LICENSE)

> **Alpha — actively seeking feedback.** Aristo is in active
> development. The code is open under MIT and stays that way.
> Day-to-day work goes to paid pilots and design partners;
> [Discussions](https://github.com/aretta-ai/aristo/discussions) are
> wide-open and feedback is what we want from you right now. Issues
> and PRs welcome.

**Verifiable intent, inline with code.** Above each Rust function, you
write a one-line claim describing what it's supposed to do. The SDK
hashes the function body alongside the claim, classifies the
verification status, and flags drift when the two diverge. You choose
how each claim gets verified — inspection, tests, formal proof, or an
AI critic — based on what the claim is worth.

Built for agent-edited codebases. Intent that used to live in the
implicit channels — author memory, code review, "the team just
knows" — has to become a first-class artifact now.
[Read the manifesto →](./docs/MANIFESTO.md)

<!-- TODO: walkthrough GIF (~60s: init → annotate → verify → drift caught → badge). See memory: aristo-readme-gif-todo. -->

> **Working on infrastructure code in Rust?** Databases, smart
> contracts, compilers, runtimes — or facing this kind of pain
> somewhere else in your codebase. If Aristo could help your team,
> [book a quick call](#). No sales, no pressure — we want to
> understand your problem so we can make Aristo better. We may not
> even be able to serve you anyway, and that's fine.

## How it works

Aristo is agent-native. Your coding agent writes and verifies intent
as it codes, using Aristo's skills; the git hook keeps the index in
step. You bring judgment — deciding which claims are worth making, and
accepting or refusing each verdict your agent brings back. The CLI
underneath is what the skills and the hook call.

The full loop — set up, write a claim, verify it, watch drift get
caught — is in **[Getting started](./docs/GETTING-STARTED.md)**.

## Install

```bash
cargo install aristo      # the `aristo` CLI
cargo add aristo          # the #[aristo::intent] / #[aristo::assume] macros
aristo install-skills --agent claude-code   # also: cursor, codex, opencode, antigravity
```

Then `aristo init` your project and code with your agent — the
[walkthrough](./docs/GETTING-STARTED.md) has the rest.

## What a claim looks like

```rust
/// Clamp `value` into the inclusive range `[lo, hi]`.
#[aristo::intent(
    "returns a value within [lo, hi] for any input",
    verify = "neural",
    id = "clamp_in_range",
)]
pub fn clamp(value: i64, lo: i64, hi: i64) -> i64 {
    value.max(lo).min(hi)
}
```

Your agent writes claims like this as it works, verifies them, and
brings each verdict back for you to accept or refuse. When the body
later drifts from the claim, the status flips to **stale** — loudly,
before it ships.

## Status

**Alpha.** The offline core is usable today and open under MIT.

- **Free, now:** writing claims, neural verification, lint, critique,
  `doc`, `graph`, the badge, and the full local index.
- **Design partners:** `verify = "full"` — the strongest checking,
  from tests up to formal proofs, via Aretta's backend.
- **In progress:** `verify = "test"` and the derived specifications it
  builds on.

Tiers climb Aspirant → Apprentice → Adept → Ascendent → Areté as a
codebase's verification deepens.

## Documentation

- **[Manifesto](./docs/MANIFESTO.md)** — why verifiable intent, and why now.
- **[Getting started](./docs/GETTING-STARTED.md)** — the agent-first walkthrough.
- **[Glossary](./docs/GLOSSARY.md)** — the vocabulary, defined once.
- **API docs** — [`aristo`](https://docs.rs/aristo) · [`aristo-core`](https://docs.rs/aristo-core) · [`aristo-macros`](https://docs.rs/aristo-macros) · [`aristo-cli`](https://docs.rs/aristo-cli)
- **CLI surface** — every `aristo <cmd> --help` is canonical.
- **[Changelog](./CHANGELOG.md)** — per-commit release notes.

## Workspace layout

```
crates/
├── aristo/         meta-crate users add via `cargo add aristo`
├── aristo-core/    shared library: types, index format, verification, language registry
├── aristo-macros/  proc-macros (#[aristo::intent], #[aristo::assume]) — intentionally thin
└── aristo-cli/     binary crate (the `aristo` CLI)
```

This repo is the canonical Rust implementation; other-language SDKs
(Python, Go, …) will shell out to this CLI, so there's one source of
truth.

## Contributing

Aristo is small, focused, and moving fast — help is welcome.

- **Questions, ideas, feedback** → [Discussions](https://github.com/aretta-ai/aristo/discussions).
- **Bugs, feature requests** → [Issues](https://github.com/aretta-ai/aristo/issues).
- **Working agreement** → [`CLAUDE.md`](./CLAUDE.md): conventional commits, small-batch ship discipline, "specifications are the truth," a CHANGELOG bullet per commit, and the pre-commit hook.

CI gates every PR: the `aristo` annotation pipeline (`stamp` / `lint` /
`verify` / `doc` checks) and standard Rust gates (`fmt`, `clippy`,
`test`, release build, `cargo doc`, MSRV 1.88).

## License

MIT — see [`LICENSE`](./LICENSE). And it stays MIT.
