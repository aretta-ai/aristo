---
name: aristo-catalogue
description: Download and browse the Aristo canon catalogue — the full corpus of canon entries (reusable invariants/properties) you can bind annotations against. Runs `aristo canon catalogue` (logged-in only) to fetch this project's instance corpus to a gitignored local snapshot at `.aristo/catalogue.json`, then reads/searches that file to answer "what canon entries exist for X?" / "is there a canon entry about Y?" / "what can I bind to?". Read-only w.r.t. source; the snapshot is a local cache, never committed.
sdk_version: {{SDK_VERSION}}
---

# Aristo catalogue

When the user invokes this skill (typically by typing `/aristo-catalogue`, or asking "what canon entries are available?", "browse the canon catalogue", "is there a canon entry about <topic>?", "what can I bind against?"), fetch the canon catalogue and answer from it.

The catalogue is the corpus of **canon entries** — the reusable, curated invariants/properties an annotation can bind to (the `aristos:` / `kanon:` tiers). This skill is **read-only** with respect to your source: it downloads a snapshot and reads it; it never stamps, binds, or edits code.

## Step 1 — download the snapshot (logged-in)

```bash
aristo canon catalogue
```

This requires sign-in — if it reports "requires authentication", tell the user to run `aristo auth login` and stop. It fetches the catalogue from this project's Aristo instance (the per-repo conductor when `[instance] url` is set in `aristo.toml`, otherwise the default server) and writes it to:

```
.aristo/catalogue.json
```

**This file is a GITIGNORED local cache.** `aristo init` adds `.aristo/catalogue.json` to `.gitignore`; it is a regenerable snapshot, **never committed**. Do NOT `git add` it, do NOT treat it as a tracked artifact or reference it in committed code/docs, and re-run `aristo canon catalogue` to refresh it. If the command reports 0 entries, the instance has no canon corpus configured (or `[instance] url` isn't pointed at a conductor) — say so; never fabricate entries.

**The snapshot's first field is a server-stamped `notice`** — a proprietary/confidential marking (this corpus is licensed to the design partner for internal use only). It is authoritative; surface it if the user asks about sharing the catalogue, never strip it, and treat it as a hard signal that this file must not leave the organization or land in any external/OSS repo.

## Step 2 — read / search the snapshot

Read `.aristo/catalogue.json` and answer the user's question from it. Each entry has:

- `canon_id` — the id to bind against (e.g. `#[aristo::intent(..., id = "kanon:<canon_id>")]`)
- `canonical_text` — the invariant/property statement
- `category`, `applies_to` — grouping + the kinds of items it applies to
- `backed_by` — scopes that back it. Non-empty ⇒ `aristos` tier (verification-backed); empty ⇒ `kanon` tier (description-only)
- `coverage_level`, `spec_refs` — backing depth + referenced spec ids

To answer "is there a canon entry about X?", search `canonical_text` / `canon_id` / `category` for the topic (substring or fuzzy). Surface the best matches with their `canon_id`, tier (aristos/kanon), and `canonical_text`. When the user is annotating and asks "what should I bind to?", prefer `aristos`-tier entries (verification-backed) whose `canonical_text` matches their intent.

## What this skill does NOT do

- It does NOT stamp, bind, or edit source — use `/aristo-authoring` to write intents and `aristo canon accept` to bind a match.
- It does NOT commit the snapshot — `.aristo/catalogue.json` is a gitignored local cache; treat it as disposable and refresh with `aristo canon catalogue`.
- It does NOT run server-side verification — it only reads the downloaded corpus listing.
