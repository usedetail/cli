---
name: cut-release
description: Cut a new release of the Detail CLI by bumping the version in Cargo.toml and merging to main. Use when asked to "cut a release", "bump the version", "publish a new version", or similar.
---

# Cut a New Release

Guide for cutting a new release of the Detail CLI. The release workflow is fully automated via GitHub Actions — all you need to do is bump the version and merge to `main`.

## How It Works

The `release.yml` workflow triggers on pushes to `main` that change `Cargo.toml`. It:
1. Reads the version from `Cargo.toml`.
2. Creates a git tag `v{version}` if one doesn't already exist.
3. Builds platform artifacts with `cargo-dist` (macOS, Linux, Windows).
4. Publishes a GitHub Release with the built artifacts and installers.

## Step 1: Decide the New Version

Always bump the **patch** version (the last number) unless the user explicitly requests otherwise:

- **Patch** (default): `0.2.4` -> `0.2.5`
- **Minor** (only if requested): `0.2.4` -> `0.3.0`
- **Major** (only if requested): `0.2.4` -> `1.0.0`

## Step 2: Bump the Version

Update the `version` field in `Cargo.toml` at the workspace root:

```toml
[package]
name = "detail-cli"
version = "X.Y.Z"   # <-- update this
```

Then run `cargo check` to ensure `Cargo.lock` is updated and everything compiles.

## Step 3: Create the PR

Create a pull request targeting `main` with:
- **Title**: `chore: release vX.Y.Z`
- **Branch**: `release/vX.Y.Z`

Wait for CI to pass (formatting, clippy, tests, vendored artifact checks).

## Step 4: Merge and Verify

Once CI is green and the PR is approved, merge it. The release workflow will automatically:
1. Create the `vX.Y.Z` tag.
2. Build all platform binaries.
3. Publish a GitHub Release.

After merging, verify the release was created at:
```
https://github.com/usedetail/cli/releases/tag/vX.Y.Z
```
