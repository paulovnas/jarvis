# Jarvis releases and updates

Jarvis publishes macOS Apple Silicon and Windows x64 together through [GitHub Releases](https://github.com/paulovnas/jarvis/releases). Beta installations accept previews and stable releases; stable installations only accept stable releases. Drafts and releases without the matching platform manifest are ignored.

## Publish from any development platform

Start from a clean, committed `main` synchronized with `origin/main`, with Bun, Git and an authenticated GitHub CLI:

```sh
bun run release 0.9.0-beta --notes-file docs/releases/0.9.0-beta.md
```

The launcher works on Windows, macOS and Linux without local signing keys, Rust or Xcode. It synchronizes the version in package.json, Tauri and Cargo, creates an annotated tag, atomically pushes the commit/tag and dispatches **Release Desktop**. The workflow filename remains `release-macos.yml` for compatibility with existing launchers.

```sh
bun run release 0.9.0-beta --dry-run
gh run list --workflow release-macos.yml
gh run watch RUN_ID --exit-status
```

Without a notes file, GitHub generates release notes. Use canonical SemVer without a leading `v` in the launcher. Previews use `-beta`; advance the version for the next preview. Published versions and existing tags are never overwritten. The launcher returning successfully means the build was scheduled; a successful publish job confirms delivery.

## CI and publication boundaries

The build matrix runs macOS Apple Silicon (`aarch64-apple-darwin`, `macos-15`) and Windows x64 (`x86_64-pc-windows-msvc`, `windows-2022`). macOS Intel, Windows ARM64 and Linux packages are not published by this matrix.

1. Validate the official repository, `main`, tag ancestry, commit and matching versions.
2. Install locked dependencies and run lint, typecheck, tests, production build, Clippy and Rust tests on both platforms. Rust tests use one test thread to avoid races around process fixtures.
3. Build the macOS app/DMG using the existing identity imported into a temporary Keychain. Build the Windows NSIS installer with the per-user installer configuration, start-menu shortcut and notification registration hooks.
4. Sign both updater packages with the existing Tauri updater key. Verify the signatures against the public key embedded in Jarvis; also verify the macOS signer fingerprint and app/DMG signatures.
5. Upload separate artifacts with platform-specific metadata and retain them for seven days. Remove temporary macOS keys even after failures.
6. A single publication job requires both builds. It validates each package's version, commit, target and signature again, creates all manifests, uploads the complete set to a draft and promotes it only after confirming every asset and its size.

Only the publication job has repository contents write permission. The existing environment `macos-release` is shared by the two build jobs to preserve its protected `main` policy and updater key. Apple secrets are provided only to macOS signing/verification steps; Windows receives the updater key only in its build step. No pull-request code runs in this workflow. Actions are pinned by commit SHA.

The Windows updater signature is **not** an Authenticode certificate. This release pipeline has no Windows publisher certificate configured, so Windows can show an unknown-publisher/SmartScreen notice. macOS signatures are preserved, but Apple notarization is not configured. Neither limitation prevents cryptographic verification by the Jarvis updater.

## Validate without publishing

In **Actions → Release Desktop → Run workflow**, use `main`, leave the tag empty and disable `publish`, or run:

```sh
gh workflow run release-macos.yml --ref main -f publish=false
```

The same checks and installers are produced as workflow artifacts. No tag or public release is created, and clients are not offered an update.

## Signing setup and recovery

The existing Mac with the original signing identity can configure the protected GitHub environment with:

```sh
bun run release:setup
```

This reads `~/.jarvis/release/updater.key` and verifies its `.pub` against `plugins.updater.pubkey`. Alternatively, `TAURI_SIGNING_PRIVATE_KEY` can point to a key file with its `.pub` beside it. Set `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` for an encrypted key.

Setup exports only the selected Apple identity to a temporary password-protected PKCS#12, sends secrets through GitHub CLI stdin and removes temporary files. macOS may request Keychain access. With multiple identities, select the original fingerprint through `APPLE_SIGNING_IDENTITY`. Setup refuses an environment allowing other branches and preserves existing protection rules.

| Secret in `macos-release` | Purpose |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Existing updater private-key contents, shared by both platforms. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Optional updater key password. |
| `APPLE_CERTIFICATE` | Original Apple identity and private key as base64 PKCS#12. |
| `APPLE_CERTIFICATE_PASSWORD` | PKCS#12 password. |
| `APPLE_SIGNING_IDENTITY` | Original certificate SHA-1 fingerprint. |

GitHub cannot return secret values after upload. Keep secure backups outside Git. Replacing the updater key breaks trust for existing installations; do not generate a new key for Windows.

## Failure and retry

A failed gate or missing platform prevents publication. Inspect Actions logs, then rerun all jobs for transient failures. If dispatch failed after push, the same launcher command can be repeated while the tag points to HEAD; omit the notes file on a retry. If `main` advanced, manually dispatch on `main` with the original tag and `publish=true`.

An incomplete draft can be resumed. Code corrections require a new version after publication. To remove a problematic release from update discovery without changing installed apps:

```sh
gh release edit vVERSION --draft
```

Publish fixes with a higher version; the updater does not downgrade.

## Published files and client behavior

Each release contains eight assets:

- macOS DMG, `.app.tar.gz` updater package and `.app.tar.gz.sig`.
- Windows `-setup.exe`, reused by the updater, and `-setup.exe.sig`.
- Combined `latest.json`, `latest-darwin-aarch64.json` and `latest-windows-x86_64.json`.

Each platform manifest contains its own package URL and signature, plus the same version, date and notes. The updater only accepts assets under the corresponding `v<version>` tag in the official repository. Build metadata stays in CI artifacts.

The update UI shows download progress and verifies the package before installation. Agents, compactions, terminals and Core installations must finish first. New executions are blocked during the update and layout preferences are persisted.

Install the first Windows version using NSIS; install the first macOS version by copying the app from the DMG to Applications. Windows uses passive NSIS updates. On macOS, the new process confirms native-window creation with the expected version and a temporary local handshake; if reopening fails, the previous window offers a retry without downloading again. A writable installed app is required; an app running from a mounted DMG cannot update in place.

Publishing does not launch or install the application locally. Unit tests cover channels/platform discovery, signed manifests, complete artifact sets, version/commit mismatches, tampering, launcher guards and update UI. A real installed A-to-B upgrade still requires validation on each destination OS.

## Reference studied

`docs/metis/docs/update-check.md` and `docs/metis/src/utils/version-check.ts` separate backend version discovery from presentation and require the manifest to match the released package. Jarvis retains those boundaries and adds Tauri's platform-specific cryptographic verification. The Metis sources remain unchanged.
