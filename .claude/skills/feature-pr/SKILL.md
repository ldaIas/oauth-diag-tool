---
name: feature-pr
description: Open a PR for the current branch and log it in CHANGELOG.md. Use when the user asks to open, raise, or create a PR for this repo's changes.
---

# Feature PR

The PR number is needed for the changelog entry, so the PR is created first as a draft and the changelog is updated afterwards.

1. **Draft PR.** Push the branch and run `gh pr create --draft`, filling in `.github/pull_request_template.md`. Leave the changelog checkbox unchecked for now. Target `main`, or the parent branch if this PR is part of a stack.
2. **Get the link.** Read the number and URL from the `gh pr create` output (or `gh pr view --json number,url`).
3. **Changelog entry.** Under `## [Unreleased]` in `CHANGELOG.md`, add one line:

   ```
   - <short description of the change> [PR](<pr url>)
   ```

   Match the style of the existing entries. Keep a single line, and nest sub-bullets only for breaking changes (mark those `**BREAKING CHANGE**`).
4. **Push the changelog.** Commit it separately (for example `update changelog for #<n>`) and push. Check the changelog box in the PR description with `gh pr edit`.
5. **Ready for review.** Run `gh pr ready` once the user confirms.

Do not bump versions or tag here. Releases are cut separately: a version-bump PR that moves `[Unreleased]` entries under the new version, then `git tag vX.Y.Z && git push origin vX.Y.Z` from `main`.
