# Releasing Aristo

Step-by-step for the user (Claude can't push to crates.io or git remote — only you can). Designed to run as one cohesive sequence; the gotchas are flagged so you can pause if something looks off.

## Prerequisites (one-time)

1. **crates.io account + API token.** Log in to https://crates.io with your GitHub account, then **Account Settings → API Tokens → New Token**. Copy the token; you'll paste it once into `~/.cargo/credentials.toml` via `cargo login <token>`. The token only needs publish scope.

2. **Crate names available.** Confirm `aristo`, `aristo-core`, `aristo-macros`, `aristo-cli` are not already taken on crates.io. Run `cargo search aristo` — if the names are claimed, pick alternative names BEFORE the version-bump commit lands.

3. **Signed-commit + signed-tag setup verified.** `git verify-commit HEAD` prints "Good signature". See README's Contributing section / your gitconfig for the SSH-signing setup. The release tag uses `git tag -s` (with `tag.gpgsign = true` set, plain `git tag v0.1.0` signs automatically).

## Branch protection setup (one-time, repo owner only)

The two CI workflows (`.github/workflows/aristo.yml` + `.github/workflows/ci.yml`) RUN on every PR and push, but their pass/fail doesn't BLOCK merging until you turn on branch protection. Do this once after the workflows have run at least once on `main` (GitHub needs to know the check names exist before it lets you require them).

**GitHub.com → Settings → Branches → Branch protection rules → Add classic branch protection rule:**

- **Branch name pattern**: `main`
- **Require a pull request before merging**: ✓
  - **Require approvals**: ✓ (set to 1 if solo or 2+ if you have a team; for solo dev set to 0 — you'll still need the PR + checks to pass)
  - **Dismiss stale pull request approvals when new commits are pushed**: ✓ (re-review after force-pushes / new commits)
- **Require status checks to pass before merging**: ✓
  - **Require branches to be up to date before merging**: ✓ (forces PR rebase on top of latest `main`)
  - **Status checks that are required** — search for and add ALL of these (names match the `name:` fields in the workflow YAMLs):
    - `aristo` (from `aristo.yml` — runs stamp / lint / verify / doc / status / badge)
    - `cargo fmt --check` (from `ci.yml::fmt`)
    - `cargo clippy` (from `ci.yml::clippy`)
    - `cargo test` (from `ci.yml::test`)
    - `cargo build --release` (from `ci.yml::build-release`)
    - `cargo doc` (from `ci.yml::docs`)
    - `cargo check (MSRV 1.75)` (from `ci.yml::msrv`)
- **Require signed commits**: ✓ (matches our local `commit.gpgsign = true` policy; rejects unsigned commits on `main`)
- **Require linear history**: ✓ (rejects merge commits; forces squash or rebase-merge — keeps `main`'s log readable)
- **Do not allow bypassing the above settings**: ✓ (applies the rules to everyone including admins; bypass should require lifting the rule, not silently circumventing)
- **Restrict who can push to matching branches**: leave empty (PRs are the only path; nothing pushes to `main` directly)

After saving, GitHub enforces these on every PR. The "Merge pull request" button stays disabled until all required checks are green AND the branch is up to date with `main`.

**If you add a new CI job later**, you must add its `name` to the required-status-checks list — the protection rule pins specific check names, not "all checks". Forgetting this is the most common branch-protection pitfall: a new failing check won't block merge because it's not in the required list.

**Verification:** open a PR with an intentionally-broken change (e.g. delete a function called from tests). The PR view should show the failing checks, and the merge button should be disabled with "Required statuses must pass before merging." If the button is enabled, the protection rule isn't covering that check name — re-check the list.

## Pre-publish sanity (run from a clean checkout)

```bash
# 1. Confirm we're on main, clean, and up to date
git checkout main
git status                                 # must be clean
git log --oneline -5

# 2. Full local test sweep (mirrors CI)
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# 3. Per-crate package dry-run (validates each tarball would build)
#    aristo-macros is the only one without workspace deps; the rest
#    can't be packaged in isolation until prior crates are published.
cargo package -p aristo-macros --no-verify --allow-dirty
ls -la target/package/aristo-macros-*.crate   # confirm tarball exists

# 4. Inspect what would be published in the macros tarball
tar -tzf target/package/aristo-macros-*.crate | head -20
```

If any of these fail, stop and fix before publishing. CI should already be green on `main`; this sanity is a belt-and-braces check.

## Publish sequence (load-bearing order)

The four crates have a dependency chain. Publishing in the wrong order will fail with "no matching package named `X` found":

```
aristo-macros           ← no workspace deps (only proc-macro2, quote, syn)
   └── aristo           ← depends on aristo-macros
          ├── aristo-core    ← depends on aristo (for proc-macros on its own annotations)
          └── aristo-cli     ← depends on aristo + aristo-core
```

Publish in dependency order:

```bash
# Each publish waits ~30s for crates.io to index the new version before
# the next crate can resolve it. Sleeps are conservative; 30s is usually
# enough but the index can lag — re-run the failing publish if needed.

cargo publish -p aristo-macros
sleep 30

cargo publish -p aristo
sleep 30

cargo publish -p aristo-core
sleep 30

cargo publish -p aristo-cli
```

After each successful publish, the version page (e.g. `https://crates.io/crates/aristo/0.1.0`) becomes live. docs.rs builds happen async (usually 5-15 minutes).

**If a publish fails mid-sequence:** crates.io has already accepted the prior publishes — you can't unpublish them (yanking is the only option, and yanked versions still occupy the version number). Fix the failing crate's issue, then `cargo publish -p <failing-crate>` from that point — don't re-run the earlier ones.

## Tag + push

```bash
# 1. Tag the release commit. `tag.gpgsign = true` signs automatically;
#    explicit -s is belt-and-braces.
git tag -s v0.1.0 -m "v0.1.0 — first public release (MVP)"

# 2. Verify the tag's signature locally
git verify-tag v0.1.0    # expects "Good signature"

# 3. Push the commits AND the tag
git push origin main
git push origin v0.1.0
```

GitHub will show ✓ Verified on both the tag page and the release commit.

## Post-publish verification

```bash
# 1. Each crate landed
cargo search aristo                       # expect 4 hits

# 2. CLI installs from crates.io (in a fresh shell or temp HOME)
cd $(mktemp -d)
cargo install aristo --locked --force
aristo --version                          # expects "aristo 0.1.0"

# 3. Library compiles in a fresh project
cargo new probe && cd probe
cargo add aristo
echo '#[aristo::intent("ones", verify = "test")] pub fn one() -> i32 { 1 }' > src/lib.rs
cargo check                               # expect success

# 4. docs.rs build status (~15 min after publish)
open https://docs.rs/aristo
open https://docs.rs/aristo-cli
open https://docs.rs/aristo-core
open https://docs.rs/aristo-macros
```

If docs.rs build fails, check **https://docs.rs/crate/aristo/0.1.0/builds** for the error. Common causes: missing default features, network-fetched assets at build time (we have none).

## Create a GitHub release

After the tag is pushed, **GitHub.com → Releases → Draft a new release**:

- **Tag**: `v0.1.0` (existing)
- **Title**: `v0.1.0 — first public release`
- **Body**: paste the `## [v0.1.0]` section from `CHANGELOG.md`
- Mark as **latest release**

This isn't load-bearing for crates.io users (they install via cargo, not GitHub), but it's the canonical changelog surface for people browsing the repo and gets included in GitHub's release feed.

## If something goes wrong

- **Wrong files in the published crate**: yank with `cargo yank --version 0.1.0 -p <crate>`. Bump to `0.1.1`, fix, re-publish. Yanked versions can't be installed but stay in the index forever — pick the next version, don't try to overwrite.
- **Bad metadata caught after publish**: same — yank + 0.1.1 patch release.
- **Signed tag wrong commit**: `git tag -d v0.1.0` locally, `git push --delete origin v0.1.0` to remove from remote, then re-tag. (The destructive push needs your explicit `--delete`; default `git push` won't override tags.)

## Future releases

For 0.1.1, 0.2.0, etc., the sequence is identical except:

1. Bump version in `Cargo.toml` (workspace.package + workspace.dependencies path-dep pins)
2. Promote `[Unreleased]` → `[v0.X.Y]` in CHANGELOG
3. Run the publish sequence above
4. Tag + push

The `cargo install aristo --locked` line in the GH workflow template (`crates/aristo-cli/src/commands/init.rs::GH_WORKFLOW_STARTER`) pins to the LATEST release automatically — no template update needed across versions.
