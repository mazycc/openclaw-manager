# Windows Offline Bundle Assets

This directory is the canonical bundle location for Windows offline install assets.
Tauri packages everything under this folder into the app bundle (`bundle.resources`).

## Expected layout

- `node/`
  - Optional Node.js installers (`*.msi`) and/or portable archives (`*.zip`)
  - Arch-specific names are auto-detected by filename (`x64`/`amd64`/`64-bit`, `arm64`)
- `git/`
  - Optional Git for Windows installer (`*.exe`)
  - Arch-specific names are auto-detected by filename
- `openclaw/`
  - Optional OpenClaw npm package tarball (`openclaw-*.tgz`, fallback: any `*.tgz`)

## Installer behavior (Windows)

- Node.js: bundled offline assets first, then existing online fallback chain.
- Git: bundled offline installer first, then winget fallback when Git is missing.
- OpenClaw: bundled local `.tgz` first, then `openclaw@latest` from npm.

If assets are absent, existing online behavior remains active.

## CI staging convention

The Windows workflows now support automated preparation:

- Local:
  - `npm run offline:windows:prepare`
- CI:
  - `Release` / `Build Windows Installer` workflow inputs
  - Or repository variables for the `Release` workflow:
    - `WINDOWS_OFFLINE_BUNDLE=true`
    - `WINDOWS_OFFLINE_ARCHITECTURES=x64`
    - `WINDOWS_OFFLINE_NODE_VERSION=latest-22`
    - `WINDOWS_OFFLINE_GIT_VERSION=latest`
    - `WINDOWS_OFFLINE_OPENCLAW_VERSION=latest`

The helper script downloads/packages assets into `offline-assets/windows/` and stages them into this folder automatically.

## Important limitation

`openclaw-*.tgz` is bundled and preferred first, but OpenClaw still has npm dependency resolution at install time.
If those dependencies are not already present in npm cache on target machines, complete air-gapped installation is not guaranteed yet.
