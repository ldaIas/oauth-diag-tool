## Summary

A brief description of what this PR does.

## Related Issue

Closes # (link to bug report or feature request, if applicable)

## Changes

-
-

## How to Test

Steps to verify the changes work as expected.

1.
2.
3.

## Checklist

- [ ] Backend changes are in Rust only
- [ ] Frontend changes are in TypeScript/React under `ui/src/` only
- [ ] New Tauri commands are registered in `specta_builder()` (`lib.rs`) and `permissions/default.toml`
- [ ] `ui/src/bindings.ts` is regenerated (`UPDATE_BINDINGS=1 cargo test bindings_are_current`) when commands or event types change
- [ ] Changelog updated - When making the PR, first create a Draft PR, get the PR number, then update the changelog with your PR #