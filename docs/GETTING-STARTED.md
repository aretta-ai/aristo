# Getting started

Aristo is agent-native. You don't drive it by hand — your coding
agent writes and verifies intent as it codes, and the git hook keeps
the index in step. This guide sets that up and walks one full loop,
including what happens when code drifts away from what it claims.

For *why* before *how*, read the [manifesto](./MANIFESTO.md). Terms
are in the [glossary](./GLOSSARY.md).

## How Aristo works with your agent

- **Your agent writes the claims.** As it writes a function, it adds
  a one-line `#[aristo::intent("…")]` above it saying what the
  function is for. (The `aristo-authoring` skill teaches it how.)
- **Your agent verifies them — and brings the results back.** It
  checks each claim against the code, then surfaces every verdict for
  you to accept or refuse. Nothing it concludes lands until you say
  so. (The `aristo-neural-verify` and `aristo-critique` skills.)
- **The git hook keeps the index honest.** Every commit re-hashes
  each function body and flags any claim whose code has drifted. You
  never run `aristo stamp` by hand.
- **You bring judgment.** You decide which claims are worth making
  and accept them into the codebase. Aristo verifies the claims you
  keep; it doesn't decide them for you.

The CLI underneath is what the skills and the hook call. You touch it
directly only to set up, and to glance at where you stand.

## One-time setup

```console
$ cargo install aristo       # the `aristo` CLI
$ cargo add aristo           # the #[aristo::intent] / #[aristo::assume] macros
```

```console
$ aristo init
ok: created aristo.toml
ok: created .aristo/index.toml (empty; 0 annotations)
ok: created .aristo/specs/
ok: created .aristo/doc/
ok: installed pre-commit hook (.git/hooks/pre-commit)
ok: wrote .github/workflows/aristo.yml (starter; edit freely)

$ aristo install-skills --agent claude-code   # also: cursor, codex, opencode, antigravity
```

That's the last time you need the terminal for the everyday loop.
`init` set up the index, the pre-commit hook, and a starter CI
workflow; `install-skills` taught your agent the authoring,
verification, and critique skills.

## The loop in practice

Now you work with your agent as usual. With the authoring skill
installed, it writes the claim alongside the code:

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

Three parts: the **claim** (what the function is for, in plain
language), the **verify mode** (`neural` — an AI critic reads the
code against the claim), and a stable **id** the agent assigns so the
claim can be tracked as the code evolves.

On commit, the hook stamps the index:

```console
→ Found 1 annotations
  new: 1, unchanged: 0, body-drifted: 0, text-changed: 0, removed: 0
ok: stamped 1 annotation into .aristo/index.toml
```

The claim is recorded but not yet verified. Your agent verifies it —
in Claude Code:

```
/aristo-neural-verify
```

The skill reads each pending claim against its function and produces
a verdict. `clamp` genuinely returns a value in `[lo, hi]`, so the
verdict is **holds** — which it brings back for you to accept. You
accept; the claim is now verified.

You didn't run a verifier. You reviewed a function, kept the claim,
and your agent did the checking.

## Drift gets caught

Here's the part that earns Aristo its place.

Later — another session, maybe another agent — a refactor quietly
drops the upper bound:

```rust
pub fn clamp(value: i64, lo: i64, hi: i64) -> i64 {
    value.max(lo)          // upper bound gone — result can exceed hi
}
```

It compiles. A shallow test might still pass. But the function no
longer does what its claim says. The next commit's stamp catches it:

```console
  new: 0, unchanged: 0, body-drifted: 1, text-changed: 0, removed: 0
  • clamp_in_range: body changed — status was Verified, now Stale
```

The verification no longer applies to this code, so the claim is
flagged **stale** — loudly, before it ships. Your agent re-runs
`/aristo-neural-verify` on the stale entry. This time the critic
reads the new body against "returns a value within [lo, hi]" and
finds it false:

> Refuted. The function returns `value.max(lo)`, which has no upper
> bound; for `value > hi` the result exceeds `hi`, violating the
> claim.

The drift didn't slip through. The claim that stopped holding got
flagged the moment the code changed, the re-check said exactly why,
and your agent brought the refutation back to you. You fix the
function — or the claim — and move on.

## Nothing lands unreviewed

Your agent doesn't quietly write its conclusions into your code — it
brings them back to you. When it finishes verifying, it surfaces each
verdict in the conversation and asks you to **accept or refuse**. You
rule on each one right there. Only what you accept gets written to
`.aristo/proofs/` and committed; the rest is dropped.

Critique runs the same handoff. The `aristo-critique` skill raises
findings on your annotation prose and presents each for the same
accept / refuse / defer call — and a finding stays open, re-surfacing
until you've ruled on it. Nothing closes on its own.

This is the workflow, not an extra step bolted on. The agent does the
labor — drafting claims, checking them, flagging weak prose — and
hands every result back for your judgment. What ends up in your repo
is a record of what you accepted, not another layer of unreviewed AI
output bloating your codebase.

## Checking in

When you want to see where a codebase stands:

```console
$ aristo status
...
Tier:
  Score:             0.050  (visible)
  Tier:              Aspirant
```

- `aristo status` — verification rate per mode, and your current tier.
- `aristo badge` — the README badge (Aspirant → Apprentice → Adept →
  Ascendent → Areté).
- `aristo show <id>` — the full record for one annotation.

These are for looking, not for driving. The everyday work stays in
your agent.

## Supported agents & integrations

Skills install into **Claude Code**, **Cursor**, **Codex**,
**OpenCode**, and **Antigravity** (`aristo install-skills --agent
<name>`). Alongside them, `aristo init` installs a **git pre-commit
hook** that re-stamps on every commit, and a **starter CI workflow**
(`.github/workflows/aristo.yml`) you can build on.

## Where to next

- **[Manifesto](./MANIFESTO.md)** — why verifiable intent, and why now.
- **[Glossary](./GLOSSARY.md)** — the vocabulary, defined once.
- **The `aristo-authoring` skill** — how your agent learns to write
  good claims, and how you shape that taste over time.
- **[Discussions](#)** — questions, ideas, what we build next. Open.
