# Live dev runbook — `dev.aretta.ai`

**What this is.** A pre-release smoke test for the §13 canon-and-matching
SDK surface against the **live** Aretta dev proxy at
`https://dev.aretta.ai`. Run it before you cut a release; run it after
any wire-affecting change lands on either side; run it whenever you
suspect drift between the SDK and the proxy.

**What this is not.** A `cargo test` replacement. The hermetic
`cargo test --workspace` suite (952 tests) covers SDK-internal
correctness against canned fixtures. This runbook covers the *wire
contract* against the real proxy — drift the hermetic tests can't
see by construction. See `../../docs/mockups/13-canon-and-matching/PLAN-live-integration-testing.md`
for the layered-testing design.

---

## How to run

```bash
./scripts/live-dev-runbook.sh              # interactive (default)
./scripts/live-dev-runbook.sh -y           # non-interactive
./scripts/live-dev-runbook.sh --non-interactive
```

The script drives the flow. Each command runs automatically; in
**interactive mode** (default) the script pauses between steps so
you can eyeball the output against the green-light criteria in this
file. Press **enter** to continue, **ctrl-c** to abort.

**Non-interactive mode** (`-y` / `--non-interactive`) skips all
pauses and the cleanup prompt — useful for CI, agents driving the
runbook, or known-good re-runs. The workspace + log are always
preserved in this mode (see the final "workspace + log preserved
at" line) so you can inspect them post-hoc. Exit code propagates
via `set -euo pipefail` — any failed step aborts with non-zero, so
`./scripts/live-dev-runbook.sh -y && echo OK` is a valid smoke
test.

**Prerequisite — auth done elsewhere.** This runbook does **not**
exercise `aristo auth login`. The auth flow has its own coverage
(four `auth_oauth_login.rs` e2e tests + the hand-validated bug fix in
`2d0cead`). Before starting:

```bash
aristo auth login --server dev --repo <owner/repo>   # one-time
aristo auth status                                    # confirm
```

If `aristo auth status` shows `server: https://dev.aretta.ai` and a
user + repo, you're good. The script will remind you and pause for
confirmation.

---

## What the script sets up

A throwaway workspace under `/tmp/aristo-runbook-<XXXX>/`:

- `aristo init` bootstrap (`.aristo/`, `aristo.toml`).
- `src/lib.rs` with **two annotations**:
  1. `be_one` — *"Waste no more time arguing what a good man should be. Be one."*
     (expected to bind to the **`kanon:`** tier — unbacked.)
  2. `obstacle_path` — *"What stands in the way becomes the way."*
     (expected to bind to the **`aristos:`** tier — backed.)

The expected tier mappings assume the dev catalog has these two
canonical texts seeded — one with verification backing (→ `aristos:`),
one without (→ `kanon:`). If dev's catalog hasn't seeded these
specific texts, the runbook fails at step 3 with "0 matches" — at
which point either (a) seed them in aretta-books and re-deploy dev,
or (b) update the runbook's source texts to ones dev has.

---

## The flow

### Step 0 — preflight

The script prints a reminder, runs `aristo auth status` to surface
what's wired up, and pauses.

**Green-light:** stdout shows `ok: authenticated via ...`, server is
`https://dev.aretta.ai`, user + repo are populated.

### Step 1 — `aristo stamp`

Walks `src/lib.rs`, computes hashes for the two annotations, calls
`POST /canon/match` with both texts, and writes
`.aristo/canon-matches.toml`.

**Green-light:**
- `ok: 2 annotations stamped, 0 ids assigned.`
- Two `→ canon-match:` lines, one per annotation.
- One line shows `(conf 0.XX, kanon: tier)` — that's `be_one`.
- One line shows `(conf 0.XX, aristos: tier)` — that's `obstacle_path`.

If both lines say the same tier, the dev catalog's seeding is wrong
for this runbook — fix the catalog (or this file) before continuing.

### Step 2 — `aristo canon list`

Reads `.aristo/canon-matches.toml` (no API call). Shows both pending
matches.

**Green-light:** both `be_one` and `obstacle_path` appear with their
canon_id, version, confidence, and tier.

### Step 3 — `aristo canon show <kanon_canon_id>`

Calls `GET /canon/entry/<id>`. Shows the canonical detail for the
kanon:-tier match (id, canonical text, applies_to, category, the
"no backing yet" framing).

**Green-light:** `backed by: — (kanon: tier; no verification backing
yet for your scope)` appears verbatim.

### Step 4 — `aristo canon show <aristos_canon_id>`

Same endpoint, the backed entry.

**Green-light:** `backed by:` shows a non-empty value (the verification
mechanism aretta-books has committed to).

### Step 5 — `aristo canon accept be_one <kanon_canon_id>`

Atomically (per accept.rs's three-step write):
1. Rewrites `src/lib.rs`'s `#[aristo::intent(...)]` — text is replaced
   with the canonical text, `id = "kanon:<canon_id>"` is added.
2. Re-keys the index under the prefixed id.
3. Moves the pending match to `accepted_matches` in the cache.

**Green-light:**
- `ok: 1 annotation bound.`
- `src/lib.rs` now has `id = "kanon:<canon_id>"` on `be_one`.

### Step 6 — `aristo show kanon:<canon_id>` (trust card)

Local read; no API call. Renders the **light** box-drawing rule per
the §13 mockup — the unbacked tier.

**Green-light:** trust-card section appears with `─` rules (light),
shows `backed by: — (no verification backing yet ...)`, and ends with
an actionable `aristo canon request-verify <bare_canon_id>` hint.

### Step 7 — `aristo canon accept obstacle_path <aristos_canon_id>`

Same as step 5 but for the backed annotation.

**Green-light:** `ok: 1 annotation bound.` + source rewritten with
`id = "aristos:<canon_id>"`.

### Step 8 — `aristo show aristos:<canon_id>` (trust card)

**Green-light:** trust-card section with `═` rules (heavy) per the
backed tier, shows `backed by:` with the verification mechanism, and
ends with the `aristo canon show <bare>` pointer for full canon-side
detail.

### Step 9 — cleanup

The script prompts: `delete /tmp/aristo-runbook-<XXXX>? [y/N]`.
**Default no** — you may want to poke around the resulting workspace
manually.

---

## Green-light summary (all must pass)

| Step | Endpoint exercised | Failure mode caught |
|---|---|---|
| 1 | `POST /canon/match` | request body shape, response decode, threshold, two-tier prefix |
| 2 | (local) | cache write shape |
| 3 | `GET /canon/entry/<id>` | URL encoding, version query, kanon: tier formatting |
| 4 | `GET /canon/entry/<id>` | aristos: tier formatting, `backed_by` field |
| 5 | (local) | source rewrite + index rekey + cache atomicity |
| 6 | (local) | trust-card kanon: variant rendering |
| 7 | (local) | accept + rewrite on the aristos: tier |
| 8 | (local) | trust-card aristos: variant rendering |

`POST /canon/request-verify` is intentionally not exercised here —
it's a Phase 2 demand-signal endpoint whose backing isn't wired up
yet (see `../../../docs/mockups/13-canon-and-matching/_deferred/`
in the meta-repo). The runbook will gain a step 9 when Phase 2
verification execution lands.

---

## When something fails

**Don't paper over it.** A red runbook is the runbook earning its
keep — that's drift between the SDK and the proxy, which is exactly
what the hermetic suite can't see.

1. Capture the failed command's full output (the script logs every
   step to `/tmp/aristo-runbook-<XXXX>/run.log`).
2. If the failure is a 5xx or transport error, retry once — `dev.aretta.ai`
   is not production-stable and transient flakes happen. Repeated
   failures are real.
3. If the failure is a 4xx or decode error, that's a wire-contract
   drift. File it on either side (aristo or aretta-code) per where
   the field rename / type change originated.
4. Once fixed: re-run the full runbook end-to-end. Partial re-runs
   miss interaction effects between steps.

The runbook's job is to be the human-readable, eyeballable counterpart
to the (forthcoming) `live_dev_*.rs` Rust shape tests. If both signal
the same drift, the drift is real and shippable to a fix; if only
the runbook signals it, the Rust tests have a shape-assertion gap
worth closing.
