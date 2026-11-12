---
name: release
description: Cut a release of this repo. Bumps the version, rolls the changelog, tags main, watches the Release workflow, and publishes the draft GitHub release. Use when the user asks to release, tag, ship, or publish a new version.
---

# Release

Pushing a `vX.Y.Z` tag triggers `.github/workflows/release.yml`. It checks that the tag matches the app version, builds installers for macOS (arm64 and x64), Linux, and Windows, signs the updater artifacts, and creates a **draft** GitHub release. The in-app updater reads `releases/latest/download/latest.json`, so the release only reaches users once the draft is published.

Pushing a tag and publishing a release are visible to users and hard to undo. Get the user's go-ahead before step 6 (push the tag) and again before step 8 (publish).

## 1. Pre-flight

- `git fetch origin --tags`. Releases are cut from `main`; do the bump on a branch off `origin/main`.
- `[Unreleased]` in `CHANGELOG.md` must have entries. If it is empty there is nothing to release.
- CI must be green on `origin/main`: `gh run list --branch main --workflow=ci.yml --limit 1`.
- The new tag must not exist locally or on the remote: `git ls-remote --tags origin vX.Y.Z`.

## 2. Pick the version

Use the version the user gave. Otherwise propose one from the `[Unreleased]` entries and the last tag (`git describe --tags --abbrev=0 origin/main`) and confirm it. The project is pre-1.0: entries marked **BREAKING CHANGE** or large feature sets bump the minor, everything else bumps the patch.

## 3. Bump the version

The workflow fails if the tag differs from either of these, so update both to `X.Y.Z` (no `v`):

- `version` in `src-tauri/tauri.conf.json`
- `version` in `src-tauri/Cargo.toml`

Then refresh the lockfile entry for `oauth-diagnostic-tool` with `cd src-tauri && cargo check` and confirm `src-tauri/Cargo.lock` changed only in that package's version. `ui/package.json` stays at `0.0.0`; it is not part of the release version.

## 4. Roll the changelog

In `CHANGELOG.md`, keep an empty `## [Unreleased]` heading at the top and put the existing entries under a new heading beneath it:

```
## [Unreleased]

## [X.Y.Z] - YYYY-MM-DD
- ...existing entries...
```

Use today's date. Do not reword the entries.

## 5. Version-bump PR

Commit the three files (`tauri.conf.json`, `Cargo.toml`, `Cargo.lock`) plus `CHANGELOG.md` as `Bump version to X.Y.Z`, push the branch, and open a PR to `main` using `.github/pull_request_template.md`. The bump PR needs no changelog entry of its own. Wait for CI, then merge once the user confirms. Never tag from the branch: the tag must point at the merge commit on `main`.

## 6. Tag

After the merge:

```bash
git fetch origin
git checkout main && git merge --ff-only origin/main
git tag vX.Y.Z
git push origin vX.Y.Z
```

Check that `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` on `main` show `X.Y.Z` before tagging.

## 7. Watch the workflow

Find the run with `gh run list --workflow=release.yml --limit 1` and follow it with `gh run watch <id> --exit-status` (a full run takes about 10 minutes).

- **`version-check` fails** (finishes in seconds): the files and the tag disagree. Delete the tag (`git push origin :refs/tags/vX.Y.Z && git tag -d vX.Y.Z`), fix the versions through another bump PR, and re-tag the new `main` commit.
- **A build leg fails**: the matrix uses `fail-fast: false`, so the other platforms still finish. Retry with `gh run rerun <id> --failed`. If the failure is a code problem, fix it on `main` and cut the next patch version instead of moving the tag.

## 8. Publish the draft

Once every job has succeeded, inspect the draft: `gh release view vX.Y.Z --json isDraft,assets --jq '{isDraft, assets: [.assets[].name]}'`. Expect `latest.json` and, for each platform, an installer plus its `.sig`:

- macOS: `.dmg` for aarch64 and x64, and an `.app.tar.gz` (with `.sig`) for each
- Linux: `.AppImage`, `.deb`, `.rpm`
- Windows: `-setup.exe` and `.msi`

Replace the placeholder release body (`See CHANGELOG.md for details.`) with the `[X.Y.Z]` section from the changelog using `gh release edit vX.Y.Z --notes-file <file>`, then publish after the user confirms:

```bash
gh release edit vX.Y.Z --draft=false --latest
```

## 9. Verify

- `curl -sL https://github.com/ldaIas/oauth-diag-tool/releases/latest/download/latest.json` returns JSON whose `version` is `X.Y.Z` and which lists every platform. This is what installed apps poll for updates.
- `gh release list --limit 1` shows `vX.Y.Z` as Latest.

Report the release URL, and anything that did not check out, to the user.
