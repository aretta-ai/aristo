#!/usr/bin/env bash
#
# live-dev-runbook.sh — drive aristo through the live-dev runbook.
#
# Companion to docs/LIVE-DEV-RUNBOOK.md. The markdown is the spec
# (what the human reads); this script is the executor (what removes
# copy-paste friction). They MUST stay in sync — when adding a step,
# update the markdown first, then mirror the change here.
#
# Auth is assumed already done. See the runbook's preflight section.
#
# Usage:
#   ./scripts/live-dev-runbook.sh

set -euo pipefail

# ─── workspace ────────────────────────────────────────────────────────────
WORKSPACE="$(mktemp -d -t aristo-runbook-XXXXXX)"
LOG="$WORKSPACE/run.log"
exec > >(tee -a "$LOG") 2>&1

# Resolve the aristo binary. Prefer a debug/release build in target/,
# fall back to PATH. Avoids requiring a global cargo-install just to
# smoke-test the local source tree.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ARISTO=""
for candidate in \
    "$REPO_ROOT/target/release/aristo" \
    "$REPO_ROOT/target/debug/aristo" \
    "$(command -v aristo || true)"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
        ARISTO="$candidate"
        break
    fi
done
if [[ -z "$ARISTO" ]]; then
    echo "error: no aristo binary found. Build with \`cargo build -p aristo-cli\` first." >&2
    exit 1
fi

cd "$WORKSPACE"

# ─── ui ───────────────────────────────────────────────────────────────────
rule() { printf '═%.0s' {1..72}; echo; }
step() {
    echo
    rule
    echo "  STEP $1 — $2"
    rule
}
say() { echo "  $*"; }
pause() {
    echo
    read -r -p "  ↪ press enter to continue (ctrl-c to abort) > " _
}
run() {
    echo
    echo "  \$ $*"
    "$@"
}

# ─── 0. preflight ─────────────────────────────────────────────────────────
step 0 "preflight"
cat <<EOF

  This runbook smoke-tests the §13 canon-and-matching SDK surface
  against the live Aretta dev proxy (https://dev.aretta.ai).

  ⚠  Auth is assumed already complete.

  Otherwise, please run:

      aristo auth login --server dev --repo <owner/repo>

  before starting this flow.

  Workspace:  $WORKSPACE
  Log:        $LOG
  Binary:     $ARISTO

EOF
pause

say "running \`aristo auth status\` to surface what's wired up:"
run "$ARISTO" auth status
pause

# ─── workspace setup ──────────────────────────────────────────────────────
step "setup" "bootstrap throwaway workspace"

mkdir -p src
cat > Cargo.toml <<'EOF'
[package]
name = "aristo-runbook-fixture"
version = "0.0.0"
edition = "2021"
publish = false

[lib]
path = "src/lib.rs"
EOF

cat > src/lib.rs <<'EOF'
#[aristo::intent(
    "Waste no more time arguing what a good man should be. Be one.",
    id = "be_one"
)]
pub fn be_one() {
    // Marcus Aurelius, Meditations 10.16 — Annotation #1.
    // Expected to bind to the `kanon:` tier (unbacked) on dev.
}

#[aristo::intent(
    "What stands in the way becomes the way.",
    id = "obstacle_path"
)]
pub fn obstacle_path() {
    // Marcus Aurelius, Meditations 5.20 — Annotation #2.
    // Expected to bind to the `aristos:` tier (backed) on dev.
}
EOF

run "$ARISTO" init

# Drop the canon thresholds so dev's `obstacle_is_the_way` match
# (conf ~0.85 against our shorter text variant) still surfaces.
# dev's catalog stores the fuller Marcus Aurelius quote
# ("The impediment to action advances action. What stands in the
#  way becomes the way."), so our shorter input lands at the edge
# of the default stamp threshold (0.85). Lowering both thresholds
# is a runbook-only convenience — production aristo.toml stays
# at the documented defaults.
cat >> aristo.toml <<'EOF'

[canon]
threshold_stamp = 0.65
threshold_critique = 0.65
EOF
echo
say "workspace ready. src/lib.rs:"
echo
sed 's/^/    /' src/lib.rs
pause

# ─── 1. stamp ─────────────────────────────────────────────────────────────
step 1 "aristo stamp — calls POST /canon/match against dev"
run "$ARISTO" stamp
echo
say "GREEN-LIGHT:"
say "  • two annotations stamped"
say "  • one match line shows  '(conf 0.XX, kanon: tier)'   — be_one"
say "  • one match line shows  '(conf 0.XX, aristos: tier)' — obstacle_path"
say "  if both show the same tier, dev's catalog seeding is wrong; fix it before proceeding."
pause

# Extract canon_ids from the canon-matches cache. More robust than
# regex-grepping stamp's pretty-printed stdout — the cache is TOML
# and the field names are stable.
CACHE=".aristo/canon-matches.toml"
if [[ ! -f "$CACHE" ]]; then
    echo "  error: $CACHE was not written. Stamp must have failed silently — abort." >&2
    exit 2
fi

# Find each annotation's first pending canon_id + tier. Cache TOML
# shape (per canon::cache::CanonMatchesFile):
#
#     [<ann_id>]                            ← top-level table
#     last_match_text_hash = "..."
#     canon_fetched_at = "..."
#
#     [[<ann_id>.pending_matches]]          ← array-of-tables
#     canon_id = "..."
#     ...
#     prefix_tier = "kanon:"
#
# The annotation id is whatever the user wrote in `id = "..."` on
# the #[aristo::intent] attribute (here: `be_one` / `obstacle_path`).
extract_canon() {
    # $1 = annotation id (e.g. "be_one")
    awk -v ann="$1" '
        # Enter the array-of-tables block: [[<ann>.pending_matches]]
        $0 ~ ("^\\[\\[" ann "\\.pending_matches\\]\\]") { in_pending = 1; next }
        # Leave on the next top-level section (table or array-of-tables).
        in_pending && /^\[/ { in_pending = 0 }
        in_pending && /^[[:space:]]*canon_id[[:space:]]*=/ && !canon {
            sub(/.*= */, ""); gsub(/"/, ""); canon = $0
        }
        in_pending && /^[[:space:]]*prefix_tier[[:space:]]*=/ && !tier {
            sub(/.*= */, ""); gsub(/"/, ""); sub(/:$/, "", $0); tier = $0
        }
        END { if (canon && tier) print canon "\t" tier }
    ' "$CACHE"
}

BE_ONE_LINE="$(extract_canon be_one)"
OBSTACLE_LINE="$(extract_canon obstacle_path)"

BE_ONE_CANON="${BE_ONE_LINE%%$'\t'*}"
BE_ONE_TIER="${BE_ONE_LINE##*$'\t'}"
OBSTACLE_CANON="${OBSTACLE_LINE%%$'\t'*}"
OBSTACLE_TIER="${OBSTACLE_LINE##*$'\t'}"

echo
say "extracted from $CACHE:"
if [[ -n "$BE_ONE_CANON" ]]; then
    say "  be_one          → canon_id=$BE_ONE_CANON tier=$BE_ONE_TIER:"
else
    say "  be_one          → (no pending match — dev returned empty results)"
fi
if [[ -n "$OBSTACLE_CANON" ]]; then
    say "  obstacle_path   → canon_id=$OBSTACLE_CANON tier=$OBSTACLE_TIER:"
else
    say "  obstacle_path   → (no pending match — dev returned empty results)"
fi

# Soft sanity check — warn but don't abort if tiers are unexpected.
if [[ -n "$BE_ONE_CANON" && "$BE_ONE_TIER" != "kanon" ]]; then
    echo
    say "⚠  expected be_one to bind to kanon: tier, got '$BE_ONE_TIER:'."
    say "   continuing anyway — eyeball subsequent steps for tier-rendering correctness."
fi
if [[ -n "$OBSTACLE_CANON" && "$OBSTACLE_TIER" != "aristos" ]]; then
    echo
    say "⚠  expected obstacle_path to bind to aristos: tier, got '$OBSTACLE_TIER:'."
    say "   continuing anyway."
fi
if [[ -z "$BE_ONE_CANON" && -z "$OBSTACLE_CANON" ]]; then
    echo
    say "⚠  No pending matches in cache. Dev's catalog returned 0 results."
    say "   Common causes: catalog state change, embedding-service blip,"
    say "   threshold too high, or the seeded entries got unloaded."
    say "   To investigate: curl dev directly (see meta-repo runbook docs)."
    say "   The subsequent canon-side steps (3-9) will be skipped."
fi
pause

# ─── 2. canon list ────────────────────────────────────────────────────────
step 2 "aristo canon list — local read"
run "$ARISTO" canon list
echo
say "GREEN-LIGHT: both be_one + obstacle_path appear under Pending."
pause

# Skip the canon-side steps if neither annotation has a pending match.
if [[ -z "$BE_ONE_CANON" && -z "$OBSTACLE_CANON" ]]; then
    say "skipping steps 3-9: no pending matches to operate on."
else
    # ─── 3. canon show — kanon: tier ──────────────────────────────────
    if [[ -n "$BE_ONE_CANON" ]]; then
        step 3 "aristo canon show $BE_ONE_CANON — GET /canon/entry/$BE_ONE_CANON"
        run "$ARISTO" canon show "$BE_ONE_CANON"
        echo
        say "GREEN-LIGHT: 'backed by: — (kanon: tier; no verification backing yet for your scope)'"
        pause
    else
        say "skipping step 3: be_one has no pending match (kanon: tier)."
    fi

    # ─── 4. canon show — aristos: tier ────────────────────────────────
    if [[ -n "$OBSTACLE_CANON" ]]; then
        step 4 "aristo canon show $OBSTACLE_CANON — GET /canon/entry/$OBSTACLE_CANON"
        run "$ARISTO" canon show "$OBSTACLE_CANON"
        echo
        say "GREEN-LIGHT: 'backed by:' shows a non-empty value (the verification mechanism)."
        pause
    else
        say "skipping step 4: obstacle_path has no pending match (aristos: tier)."
    fi

    # ─── 5. accept kanon: ────────────────────────────────────────────
    if [[ -n "$BE_ONE_CANON" ]]; then
        step 5 "aristo canon accept be_one $BE_ONE_CANON"
        run "$ARISTO" canon accept be_one "$BE_ONE_CANON"
        echo
        say "GREEN-LIGHT:"
        say "  • 'ok: 1 annotation bound.'"
        say "  • src/lib.rs now has  id = \"kanon:$BE_ONE_CANON\"  on be_one:"
        echo
        sed -n '1,8p' src/lib.rs | sed 's/^/    /'
        pause

        # ─── 6. trust card — kanon: ──────────────────────────────────
        step 6 "aristo show kanon:$BE_ONE_CANON — trust card (light rule)"
        run "$ARISTO" show "kanon:$BE_ONE_CANON"
        echo
        say "GREEN-LIGHT:"
        say "  • trust-card section with light box rules (─)"
        say "  • 'backed by: — (no verification backing yet ...)'"
        say "  • actionable hint: aristo canon request-verify $BE_ONE_CANON"
        pause
    else
        say "skipping steps 5-6: be_one has no pending match."
    fi

    # ─── 7. accept aristos: ──────────────────────────────────────────
    if [[ -n "$OBSTACLE_CANON" ]]; then
        step 7 "aristo canon accept obstacle_path $OBSTACLE_CANON"
        run "$ARISTO" canon accept obstacle_path "$OBSTACLE_CANON"
        echo
        say "GREEN-LIGHT:"
        say "  • 'ok: 1 annotation bound.'"
        say "  • src/lib.rs now has  id = \"aristos:$OBSTACLE_CANON\"  on obstacle_path."
        pause

        # ─── 8. trust card — aristos: ────────────────────────────────
        step 8 "aristo show aristos:$OBSTACLE_CANON — trust card (heavy rule)"
        run "$ARISTO" show "aristos:$OBSTACLE_CANON"
        echo
        say "GREEN-LIGHT:"
        say "  • trust-card section with heavy box rules (═)"
        say "  • 'backed by:' shows the verification mechanism"
        say "  • pointer: aristo canon show $OBSTACLE_CANON"
        pause
    else
        say "skipping steps 7-8: obstacle_path has no pending match."
    fi

    # ─── 9. request-verify (idempotency) ─────────────────────────────
    # Pick whichever canon_id we have; prefer the kanon: tier since
    # request-verify makes the most sense for unbacked entries.
    RV_CANON="${BE_ONE_CANON:-$OBSTACLE_CANON}"
    if [[ -n "$RV_CANON" ]]; then
        step 9 "aristo canon request-verify $RV_CANON — first call (submitted)"
        run "$ARISTO" canon request-verify "$RV_CANON" --notes "live runbook smoke test"
        echo
        say "GREEN-LIGHT: 'submitted' (Aretta has been notified)."
        pause

        step "9b" "aristo canon request-verify $RV_CANON — repeat call (updated)"
        run "$ARISTO" canon request-verify "$RV_CANON" --notes "live runbook smoke test — round 2"
        echo
        say "GREEN-LIGHT: 'updated' (idempotent path)."
        pause
    fi
fi

# ─── cleanup ─────────────────────────────────────────────────────────────
step "done" "all green-lights cleared"
echo
say "workspace + log preserved at: $WORKSPACE"
say ""
read -r -p "  delete $WORKSPACE? [y/N] > " ans
if [[ "$ans" =~ ^[Yy]$ ]]; then
    rm -rf "$WORKSPACE"
    echo "  removed."
else
    echo "  kept. Poke around as needed."
fi
