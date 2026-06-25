# ADR: `.aristo/index.toml` is a gitignored local cache; status is sourced from `.aristo/proofs/`

**Status:** Accepted (Option B). Implementation staged across slices 1 to 7.
**Supersedes (by reference, per "specs are immutable"):** meta `DECISIONS.md` **D1** (index committed), **G8** (status as an index-resident field), **B3** (staleness model), **J5** (mtime freshness preflight). Those records are not edited; this ADR is the new authority.

## Context

Today `.aristo/index.toml` is committed to git and a pre-commit hook runs `aristo stamp` on every commit to keep it in lockstep with source. That coupling is the source of real friction:

- The pre-commit hook lives in `.git/hooks/`, which git never tracks, so a fork contributor gets no hook. The hook also hard-calls `aristo` under `set -e`, so a contributor without the binary has every commit aborted by `command not found`.
- `stamp --check` / `doc --check` red-fail PRs on committed-artifact drift the contributor cannot fix without the binary.
- The index merge-conflicts on every code-touching PR because the derived hashes live in the same committed file as the carried verdict.

The key realization: **the committed index is already a pure cache.** Every `IntentEntry` field is either a pure function of source (`text`, `text_hash`, `body_hash`, `file`, `site`, `covered_region`, `parent`, `verify`) or carried verification state that has a durable home **outside** the index:

- `status` is a denormalized cache of the verdict. The durable record is `.aristo/proofs/<id>.proof`, and `verify/mod.rs` already declares "the validator is the source of truth; the status flag is a cache."
- `binding` is re-derived from `.aristo/canon-matches.toml` on every `stamp` (`derive_bindings_from_cache`); the index column was never authoritative.
- The `Certified` cert triple (`verified_outcome` + `last_verified_at_commit`) is **never constructed by live code** today (test fixtures only). The verify=full server dispatch runs but persists nothing locally.
- The critique-cache markers (`last_critiqued_at_text_hash`, `last_critique_finding_count`) are a skip-optimization whose backing `.critique` files are gitignored, so they are already clone-moot.

Therefore the index can become a gitignored, regenerable local cache with **no loss of any guarantee**, provided the read path can regenerate it in memory and source `status` from the proofs.

## Decision

1. **`.aristo/index.toml` becomes gitignored.** It is a local convenience cache, not a tracked artifact.
2. **`aristo stamp` / `aristo index` stay, but become optional.** They refresh the local cache and archive orphan proofs; nothing requires them to run.
3. **Read commands call an in-memory regeneration directly** rather than depending on a committed index. A new `load_index(ws)` prefers a fresh local cache and otherwise regenerates from source + proofs + canon-matches.
4. **`status` is sourced from `.aristo/proofs/`** via `merge_status_from_proofs`: anchors valid -> the proof's verdict; anchors drifted -> `Stale`; no proof -> `Unknown`.
5. **Freshness is enforced in CI by regenerate-and-audit-proofs**, not by checking a committed index is in sync.

The durable, git-tracked state after this change: in-source annotations + `.aristo/proofs/*.proof` (verdict + anchors) + `.aristo/expectations.toml` (waivers) + `.aristo/doc/*` (compile-time `include_str!`) + `.aristo/feedback/` + `.aristo/canon-matches.toml`.

## Resolved load-bearing facts (verified against `aristo` @ v0.2.9)

1. **The annotation `id` is not in the hash preimage.** `extract.rs:193` computes `text_hash(&text)` and `body_hash(&body)`; neither takes the id. A pure id-rename leaves both anchors unchanged, so a moved `.proof` still anchor-validates. This is what makes `rename` safe under Option B.
2. **`derived_status` (apply.rs:283) emits only `{Neural, Counterexample, Inconclusive}`.** `VerdictType` has exactly `{Verified, Counterexample, Inconclusive}`, with `Verified -> Neural`. (verify=test currently rides `VerdictType::Verified -> Neural`, a pre-existing quirk, not introduced here.)
3. **The live status universe is `{Unknown, Neural, Counterexample, Inconclusive, Stale}`.** `Orphan`/`Forged`/`PendingDeepen`/`Tested`/`Verified` are unreachable in current code (test/display only). The proofs-join reproduces 100% of the live universe.
4. **No live writer of `BindingState::Certified` / `verified_outcome` / `last_verified_at_commit`.** Every occurrence is a test fixture or a read-side destructure.

## 1. The core seam

### 1.1 `load_index` (replaces `read_index`)

Lives in `crates/aristo-cli/src/commands/show.rs` (or a promoted `index_load.rs`). Prefers a fresh local cache; otherwise regenerates in memory. Never errors on a missing index.

```rust
pub(crate) fn load_index(ws: &Workspace) -> CliResult<IndexFile> {
    if let Some(idx) = read_local_cache_if_present(ws)? { return Ok(idx); }
    regenerate_index(ws)
}

pub(crate) fn regenerate_index(ws: &Workspace) -> CliResult<IndexFile> {
    let walk_opts = walk_options_from_workspace(ws)?;
    let discovered = walk_directory_with(&ws.root, &walk_opts)?;
    let (mut entries, parents_map) = index::build_entries(&discovered, &ws.root)?;
    detect_cycles(&parents_map)?;
    let cache = CanonMatchesFile::read(&ws.canon_matches_path())?;
    stamp::derive_bindings_from_cache(&mut entries, &cache);
    // Build Meta inline with the CURRENT schema_version (no phantom Meta::current).
    let meta = Meta { schema_version: 1, generated_by: Some(...), generated_at: Some(now_rfc3339()), source_root: Some(".".into()) };
    let mut index = IndexFile { meta, entries };
    stamp::merge_status_from_proofs(&mut index, ws)?;
    Ok(index)
}
```

`derived_status` (apply.rs:283) must be promoted `private -> pub(crate)`. `validate`, `build_entries`, `derive_bindings_from_cache` are already `pub(crate)`.

### 1.2 `merge_status_from_proofs` (replaces `merge_status_from_prev`)

Lives in `crates/aristo-cli/src/commands/stamp.rs`. Two phases:

- **Phase 1 (focal anchor check, no cross-entry dependency):** for each entry, load `pending::proof_path_for(ws, id)`; if the proof's `produced_at_text_hash`/`produced_at_body_hash` equal the entry's current hashes -> `derived_status(pf)`, else `Stale`; no proof -> leave `Unknown`.
- **Phase 2 (full validate against a snapshot carrying Phase-1 statuses):** re-run `validator::validate` so the refuted-sibling-ground guard (`validator.rs:541`) fires; demote to `Stale` on any report.

Two-phase is load-bearing: running the full validator while every entry is still `Unknown` would let a proof launder grounding in a refuted sibling.

This subsumes both halves of `merge_status_from_prev` (the prior-status carry and the body/text-drift -> Stale demotion), because the anchor check **is** the drift detector.

### 1.3 Performance

Every read now pays a source walk + N proof parses (N bounded by terminal-proof count). Acceptable for `list/metrics/status/badge/doc/graph`. **Statusline is the exception** (must never walk per render): `load_index` keeps the local cache as the hot path behind a cheap mtime gate, and statusline must **degrade-to-cache (never walk per render)**, refreshed only by `stamp`/`verify`.

## 2. Writer disposition

| Writer | Under Option B |
|---|---|
| **stamp** | Writes the gitignored local cache only; never required. `merge_status_from_prev` -> `merge_status_from_proofs`. `archive_orphan_proofs` redefined as set-difference of on-disk proof ids minus walked entry ids. |
| **verify/apply** | Index-status write becomes conditional (local-cache-only). The durable record is the `.proof`, already stamped before the index write. |
| **critique/apply** | Drops the index write. `critique_is_current` reads the `.critique` file's `critiqued_at_text_hash`. The two `IntentEntry` cache fields are deleted. |
| **canon/accept, unbind** | Drop the index write; keep the durable source id-prefix rewrite + canon-matches update. Binding reconstructs via `derive_bindings_from_cache`. |
| **canon/reject, migrate, refresh, suggestions** | Readers; swap to `load_index`. `suggestions::local_state` must drop its silent-empty `if index_path.is_file()` guard or it under-suppresses. |
| **rename** | Drops the index leg; keeps source id-edit + `.proof`/`.critique` re-key. **Reverse leg order: re-key the `.proof` before the source edit**, so a crash leaves a detectable orphan proof, not a silently-downgraded verified annotation. |
| **lint/fix** | Unaffected (never touches the index). |

## 3. State that needs a new home first

**verify=full `Certified` cert** (`verified_outcome` + `last_verified_at_commit`) is the only index-only carried state with no durable home and no regeneration path. **Currently unreachable, so safe to defer.** The accurate framing (corrected from "unreachable"): the verify=full dispatch **is reachable today but persists no local status** (server-snapshot only), so gitignoring the index loses nothing it was not already losing.

**Hard sequencing gate:** before any verify=full verdict is ever persisted locally, it must target a `.proof`-style receipt (a `method = "full"` verdict carrying the opaque server cert + commit hash), not the index. The `ProofFile`/`VerdictMeta` schema has no cert slot today; that addition must land with full-cert persistence. Ship a failing-skipped invariant test: **no `Certified` binding and no full verdict is ever written to index status without a `.proof` receipt.**

**Critique-cache markers** recompute from the `.critique` file (which already embeds `critiqued_at_text_hash` + findings). Ship in this migration; delete the two `IntentEntry` fields per "make bad states unrepresentable."

## 4. Wiring

- **gitignore:** add the single targeted line `.aristo/index.toml` (never `.aristo/`, which would untrack `proofs/`/`doc/`/`expectations.toml` and break `cargo doc`'s `include_str!`). `git rm --cached .aristo/index.toml` committed. Rewrite the `.gitignore` rationale block.
- **init:** keep creating a local `index.toml` cache (zero `git status` surprise once gitignored); rewrite the module doc; `aristo init` writes/patches the consumer `.gitignore` with the index line.
- **CI:** replace `aristo.yml` `stamp --check` with `verify --audit --strict`. **Strictness policy (corrected, M2):** `--strict` must red on `Stale`, `Counterexample`, orphan proof, **and `Unknown`-with-a-verify-level-declared, and any `.proof` deletion.** Without that, a careless or malicious PR hides a regression the committed-index diff would have shown. Keep `doc --check` and `lint --check` (both self-regenerate via `load_index`; `doc --check` stays byte-stable because default rendering is status-free).
- **preflight / J5:** delete `freshness_check` + `emit_advisory_if_stale` + all call sites + the mtime plumbing in statusline. Proof-anchor drift (via `verify --audit` / `merge_status_from_proofs`) is the real freshness signal.
- **pre-commit hook:** prepend `command -v aristo >/dev/null 2>&1 || exit 0`; demote `aristo stamp` to optional local-cache refresh; keep `aristo doc` (load-bearing for `include_str!`).
- **cross-repo:** `aretta-ai/aristo-action` must redefine its `stamp` check vocabulary in lockstep.

## 5. Ordered slice plan

The index stays **committed** until the read funnel + status-from-proofs + CI gate are in place; the gitignore flip is **last**. Each slice is independently shippable and green.

1. **`merge_status_from_proofs` behind the scenes** (no behavior flip). Add the two-phase logic; promote `derived_status`; wire a compute-both divergence logger in `stamp`. Test the **refinement property**, not equality (corrected, B2): the join may legitimately differ from the prev-index path on the Stale-then-reverted case; it must never report terminal-clean where the prev path reported not-clean.
2. **`load_index` + `regenerate_index`; swap pure-derived readers** (`lint`, `doc` default, `graph` default). Delete the `show.rs` hard-error. Verify `doc --check` stays byte-identical.
3. **Swap status-dependent readers** (`list`, `metrics`, `status`, `review`, `show`, `--include-status`, `nudge`, `badge`, `statusline` with degrade-to-cache).
4. **verify cluster + writer relaxation.** Swap `verify`; make `apply_status_updates` conditional; flip `merge_status_from_prev -> merge_status_from_proofs` for real; redefine `archive_orphan_proofs`; add `verify --audit [--strict]`.
5. **canon + rename + critique writer disposition.** Drop redundant index writes; reverse rename leg order; delete the two `IntentEntry` critique fields; redefine `critique_is_current`.
6. **CI gate flip.** `stamp --check -> verify --audit --strict`; hook `command -v` guard; delete preflight/J5.
7. **The gitignore flip (last).** `.gitignore` += `.aristo/index.toml`; `git rm --cached`; rewrite rationale; init writes the consumer line. **Add a CI grep gate (corrected, M4): zero non-cache `read_index(` callers remain (the real count is 33 references, not 20).**

(verify=full cert durable-home, §3, is a separate Phase-2-gated prerequisite, not on this critical path.)

## 6. Trust-model note (do not overclaim)

The proofs-join **preserves**, it does not strengthen, the trust model. Neural verdicts are worker-asserted, structurally validated, and anchor-pinned under both the committed-index and proofs-join models; the validator checks proof-tree structure + anchors + grounds, it does not re-run the LLM. The only enforcement gain is freshness (anchor drift -> Stale), which already existed. Option B introduces **no new forge surface** and removes none.

## Consequences

- **Gained:** the fork-PR showstopper, the binary-required-to-commit, the index merge-conflict class, and the "you forgot to regenerate" red-CI all disappear. The trust model's polarity is corrected (durable state committed, derived cache gitignored).
- **Traded:** the committed-index review diff (reconstruct via `verify --audit --summary`), hand-editability of the index, and the single-file project view. The read path moves from "parse a file" to "regenerate-or-cache".
- **Conditional:** freshness enforcement is preserved only if the §4 `verify --audit --strict` policy (Unknown-with-verify-level + proof-deletion) is built. Skipping it yields a weaker gate than today.
