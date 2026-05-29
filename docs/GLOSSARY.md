# Glossary

The vocabulary Aristo uses, defined once. If a term in the
[manifesto](./MANIFESTO.md), the README, or the CLI isn't clear,
it's here.

## The words behind the name

**Aretê** (ἀρετή) — Classical Greek for excellence as habituated
practice: the virtue of the craftsperson who keeps showing up. The
root idea behind the project — and the name of the top badge tier.

**Aristos** (ἄριστος) — The Greek superlative: the best, the finest.
The source of the product name.

**Aristo** — The open-source SDK: the annotation macros, the CLI, the
index format, and the verification orchestration. MIT-licensed. What
you install with `cargo install aristo`.

**Aretta** — The company behind Aristo. Maintains the SDK and runs the
paid backend for deeper machine-checking and hosted solutions.

## The practice

**Annotation** — A natural-language claim written above a function,
stating what it's supposed to do. You or your agent writes it; you
exercise judgment about which claims are worth making and accept them
into the codebase. The human-readable artifact — and the one place
your thinking still does the work.

**Intent** — An annotation attached with `#[aristo::intent("…")]`: a
claim about what a function does. The common case.

**Assumption** — An annotation attached with `#[aristo::assume("…")]`:
a condition the function relies on rather than guarantees.
Documentation-only by design — assumptions are recorded, never
verified.

**Specification** *(on the roadmap)* — The machine-checkable artifact
derived from an annotation, stored under `.aristo/specs/`. Where an
*annotation* is prose a human reads, a *specification* is what the
stronger verify modes check against. The two are deliberately
distinct.

**The verify spectrum** — How rigorously a claim is checked, set per
annotation:
- `verify = false` — a claim only. Useful documentation; no
  verification.
- `verify = "neural"` — an AI critic reads the function against the
  claim and renders a verdict.
- `verify = "test"` *(in progress)* — the claim is checked against
  your existing tests, augmented with a derived assertion. Builds on
  the specification work above.
- `verify = "full"` *(design partners)* — the **strongest** mode: the
  server does the best check it can, escalating from tests up to full
  formal proofs.

**Staleness** — When a function body changes after its claim was last
verified, the claim is no longer trustworthy. Aristo detects this by
hashing the function body; when the hash drifts, the verification
status reverts to *unknown*.

**The index** — `.aristo/index.toml`, committed to your repo. The
machine-readable record of every annotation: id, hash, verify mode,
and current verification status.

## Trust signals

**The tiers** — A codebase's verification maturity, summarized as one
grade on a path:
- **Aspirant** — seeking the path; has annotations, minimal
  verification.
- **Apprentice** — learning the practice; lint and critique pass.
- **Adept** — demonstrating skill; meaningful verification coverage.
- **Ascendent** — rising toward areté; near-full verification. The
  free-tier ceiling.
- **Areté** — excellence achieved. Hidden until reached; gated on
  server-bound formal proofs.

**Badge** — The shields.io-style SVG (`aristo badge`) that renders a
project's current tier. The public face of the practice.

**The `aristos:` namespace** — The id prefix marking a server-bound
annotation: one whose verification carries a certificate issued by
Aretta's backend. Reaching the Areté tier requires proofs in this
namespace.
