#!/usr/bin/env sh
# Drift check for the generated conductor route list (T0 "names once").
#
# Regenerates crates/aristo-core/src/conductor/paths.rs from the vendored spec registry
# (spec/v0.2.239/) with the spec's own generator and the PINNED rustfmt (Rust 1.96.0's), and fails
# on any difference to the committed file. Never edit paths.rs by hand: change the registry in
# the registry (CampanileSkyForge/aretta-registry, data/), re-vendor it, and regenerate.
#
#   scripts/check-conductor-paths.sh            check (exit 1 on drift)
#   scripts/check-conductor-paths.sh --write    regenerate paths.rs in place
#
# Needs python3 with PyYAML and RUSTFMT pointing at rustfmt 1.9.0 (ac68faa20c 2026-05-25).
set -eu
root="$(cd "$(dirname "$0")/.." && pwd)"
spec="$root/spec/v0.2.239"
committed="$root/crates/aristo-core/src/conductor/paths.rs"
RUSTFMT=${RUSTFMT:-rustfmt}
PIN='(ac68faa20c 2026-05-25)'   # rustfmt of Rust 1.96.0, the generator's pin
v=$("$RUSTFMT" --version)
case "$v" in *"$PIN"*) ;; *) echo "check-conductor-paths: rustfmt is '$v', the pin is 1.96.0's $PIN (set RUSTFMT)" >&2; exit 2;; esac

out=$(mktemp -d "${TMPDIR:-/tmp}/conductor-paths.XXXXXX")
trap 'rm -rf "$out"' EXIT
# The generator also writes enum schemas and a CI stub next to the registry, so it runs on a copy:
# spec/ keeps exactly the files its .sha256 lists.
cp -R "$spec" "$out/spec"
python3 -B "$out/spec/scripts/generate-routes.py" --registry "$out/spec/routes/registry.yaml" --out "$out"
fresh="$out/cli/crates/aristo-core/src/conductor/paths.rs"
"$RUSTFMT" --edition 2021 "$fresh"

if [ "${1:-}" = "--write" ]; then
  cp "$fresh" "$committed"
  echo "wrote $committed from spec/v0.2.239/routes/registry.yaml"
  exit 0
fi
if ! cmp -s "$fresh" "$committed"; then
  echo "check-conductor-paths: crates/aristo-core/src/conductor/paths.rs is not what spec/v0.2.239/routes/registry.yaml generates" >&2
  diff -u "$committed" "$fresh" >&2 || true
  exit 1
fi
echo "crates/aristo-core/src/conductor/paths.rs is spec/v0.2.239's"
