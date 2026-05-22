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
#[aristo::intent("Waste no more time arguing what a good man should be. Be one.")]
pub fn be_one() {
    // Marcus Aurelius, Meditations 10.16 — Annotation #1.
    // Expected to bind to the `kanon:` tier (unbacked) on dev.
}

#[aristo::intent("What stands in the way becomes the way.")]
pub fn obstacle_path() {
    // Marcus Aurelius, Meditations 5.20 — Annotation #2.
    // Expected to bind to the `aristos:` tier (backed) on dev.
}
EOF

run "$ARISTO" init
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

# Find each annotation's first pending canon_id + tier.
extract_canon() {
    # $1 = annotation id (e.g. "be_one")
    awk -v ann="$1" '
        $0 ~ ("^\\[entries\\.\"" ann "\"\\]") { in_entry = 1; next }
        in_entry && /^\[entries\./ { in_entry = 0 }
        in_entry && /^\s*canon_id\s*=/ && !canon { sub(/.*= */, ""); gsub(/"/, ""); canon = $0 }
        in_entry && /^\s*prefix_tier\s*=/ && !tier { sub(/.*= */, ""); gsub(/"/, ""); tier = $0 }
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
say "  be_one          → canon_id=$BE_ONE_CANON   tier=$BE_ONE_TIER"
say "  obstacle_path   → canon_id=$OBSTACLE_CANON tier=$OBSTACLE_TIER"

# Soft sanity check — warn but don't abort if tiers are unexpected.
# The user may want to continue manually to see what failed.
if [[ "$BE_ONE_TIER" != "kanon" ]]; then
    echo
    say "⚠  expected be_one to bind to kanon: tier, got '$BE_ONE_TIER:'."
    say "   continuing anyway — eyeball subsequent steps for tier-rendering correctness."
fi
if [[ "$OBSTACLE_TIER" != "aristos" ]]; then
    echo
    say "⚠  expected obstacle_path to bind to aristos: tier, got '$OBSTACLE_TIER:'."
    say "   continuing anyway."
fi
pause

# ─── 2. canon list ────────────────────────────────────────────────────────
step 2 "aristo canon list — local read"
run "$ARISTO" canon list
echo
say "GREEN-LIGHT: both be_one + obstacle_path appear under Pending."
pause

# ─── 3. canon show — kanon: tier ──────────────────────────────────────────
step 3 "aristo canon show $BE_ONE_CANON — GET /canon/entry/$BE_ONE_CANON"
run "$ARISTO" canon show "$BE_ONE_CANON"
echo
say "GREEN-LIGHT: 'backed by: — (kanon: tier; no verification backing yet for your scope)'"
pause

# ─── 4. canon show — aristos: tier ────────────────────────────────────────
step 4 "aristo canon show $OBSTACLE_CANON — GET /canon/entry/$OBSTACLE_CANON"
run "$ARISTO" canon show "$OBSTACLE_CANON"
echo
say "GREEN-LIGHT: 'backed by:' shows a non-empty value (the verification mechanism)."
pause

# ─── 5. accept kanon: ────────────────────────────────────────────────────
step 5 "aristo canon accept be_one $BE_ONE_CANON"
run "$ARISTO" canon accept be_one "$BE_ONE_CANON"
echo
say "GREEN-LIGHT:"
say "  • 'ok: 1 annotation bound.'"
say "  • src/lib.rs now has  id = \"kanon:$BE_ONE_CANON\"  on be_one:"
echo
sed -n '1,5p' src/lib.rs | sed 's/^/    /'
pause

# ─── 6. trust card — kanon: ──────────────────────────────────────────────
step 6 "aristo show kanon:$BE_ONE_CANON — trust card (light rule)"
run "$ARISTO" show "kanon:$BE_ONE_CANON"
echo
say "GREEN-LIGHT:"
say "  • trust-card section with light box rules (─)"
say "  • 'backed by: — (no verification backing yet ...)'"
say "  • actionable hint: aristo canon request-verify $BE_ONE_CANON"
pause

# ─── 7. accept aristos: ──────────────────────────────────────────────────
step 7 "aristo canon accept obstacle_path $OBSTACLE_CANON"
run "$ARISTO" canon accept obstacle_path "$OBSTACLE_CANON"
echo
say "GREEN-LIGHT:"
say "  • 'ok: 1 annotation bound.'"
say "  • src/lib.rs now has  id = \"aristos:$OBSTACLE_CANON\"  on obstacle_path."
pause

# ─── 8. trust card — aristos: ────────────────────────────────────────────
step 8 "aristo show aristos:$OBSTACLE_CANON — trust card (heavy rule)"
run "$ARISTO" show "aristos:$OBSTACLE_CANON"
echo
say "GREEN-LIGHT:"
say "  • trust-card section with heavy box rules (═)"
say "  • 'backed by:' shows the verification mechanism"
say "  • pointer: aristo canon show $OBSTACLE_CANON"
pause

# ─── 9. request-verify (idempotency) ─────────────────────────────────────
step 9 "aristo canon request-verify $BE_ONE_CANON — first call (submitted)"
run "$ARISTO" canon request-verify "$BE_ONE_CANON" --notes "live runbook smoke test"
echo
say "GREEN-LIGHT: 'submitted' (Aretta has been notified)."
pause

step "9b" "aristo canon request-verify $BE_ONE_CANON — repeat call (updated)"
run "$ARISTO" canon request-verify "$BE_ONE_CANON" --notes "live runbook smoke test — round 2"
echo
say "GREEN-LIGHT: 'updated' (idempotent path)."
pause

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
