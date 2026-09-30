# Jarvis releases and updates

The release pipeline publishes macOS Apple Silicon, Windows x64 and Linux x64 together through [GitHub Releases](https://github.com/paulovnas/jarvis/releases). Linux is a new target; package availability starts with the first successful release containing this implementation. Beta installations accept previews and stable releases; stable installations only accept stable releases. Drafts and releases without the matching platform manifest are ignored.

## Publish from any development platform

Start from a clean, committed `main` synchronized with `origin/main`, with Bun, Git, an authenticated GitHub CLI, Rust with Clippy and the native build prerequisites for the local platform (Xcode Command Line Tools on macOS, Visual Studio C++ Build Tools on Windows, GTK/WebKitGTK development packages on Linux):

```sh
bun run release 0.9.0-beta --notes-file docs/releases/0.9.0-beta.md
```

The launcher works on Windows, macOS and Linux without local signing keys. It synchronizes the version in package.json, Tauri and Cargo, creates the version commit and runs locked dependency installation, `bun run check`, `cargo clippy --locked --all-targets -- -D warnings` and the complete Rust test suite locally. Rust tests use two threads. Only after these pass and HEAD, branch and working tree remain unchanged does it create an annotated tag recording the validated commit, atomically push that exact commit/tag and dispatch **Release Desktop**. The workflow filename remains `release-macos.yml` for compatibility with existing launchers.

The `Jarvis-Local-Checks-v1` tag trailer is the maintainer's declaration that all local gates passed for its SHA; it is not a remote test run or a signed attestation. It is excluded from public release notes. There is no skip-checks flag. A retry can reuse the declaration only while the tag points to the same validated commit.

```sh
bun run release 0.9.0-beta --dry-run
gh run list --workflow release-macos.yml
gh run watch RUN_ID --exit-status
```

Without a notes file, GitHub generates release notes. Use canonical SemVer without a leading `v` in the launcher. Previews use `-beta`; advance the version for the next preview. Published versions and existing tags are never overwritten. The launcher returning successfully means the build was scheduled; a successful publish job confirms delivery.

## CI and publication boundaries

The build matrix runs macOS Apple Silicon (`aarch64-apple-darwin`, `macos-15`), Windows x64 (`x86_64-pc-windows-msvc`, `windows-2022`) and Linux x64 (`x86_64-unknown-linux-gnu`, `ubuntu-22.04`). The older Ubuntu builder limits the glibc baseline; it does not qualify every distribution or desktop. macOS Intel, Windows ARM64, Linux ARM64 and RPM are outside this matrix.

1. Validate the official repository, `main`, tag ancestry, commit and matching versions.
2. Require the tag's local validation declaration to match the source SHA before publication. The workflow first loads the launcher from its main revision, then selects the requested source tag, so old source scripts cannot bypass this gate. Install locked dependencies and prepare the native toolchain. The release matrix does not repeat lint, the complete frontend/Rust test suites or Clippy; Tauri still compiles the frontend and native application required for each installer.
3. Build the macOS app/DMG using the existing identity imported into a temporary Keychain. Build the Windows NSIS installer with the per-user installer configuration, start-menu shortcut and notification registration hooks. Build Linux DEB and AppImage with GTK/WebKitGTK and D-Bus prerequisites; DEB declares its D-Bus dependency and recommends a credential service and Bubblewrap.
4. Sign all updater packages with the existing Tauri updater key. Verify the signatures against the public key embedded in Jarvis; also verify the macOS signer fingerprint and app/DMG signatures. Linux uses the signed AppImage for updates; the DEB is a separate installer.
5. Upload separate artifacts with platform-specific metadata and retain them for seven days. Remove temporary macOS keys even after failures.
6. A single publication job requires all three builds. It validates each package's version, commit, target and updater signature again, creates all manifests, uploads the complete set to a draft and promotes it only after confirming every asset and its size.

Only the publication job has repository contents write permission. The existing environment `macos-release` is shared by the three build jobs to preserve its protected `main` policy and updater key. Apple secrets are provided only to macOS signing/verification steps; Windows and Linux receive the updater key only in their build step. No pull-request code runs in this workflow. Actions are pinned by commit SHA.

**Native integrations** (`native-validation.yml`) is a separate, read-only CI workflow for changes to native sources, scripts, manifests, lockfiles or workflows on pushes to main and pull requests. It runs focused suites for processes, terminals, shell/sandbox, credentials, notifications, MCP subprocesses, Claude CLI and updates on the same three operating systems. Linux also runs the isolated Secret Service test with a disposable D-Bus session and GNOME Keyring wallet. Generic agent/provider and frontend suites remain local. A comparison of the changed manifests skips commits that only update Jarvis's version; dependency or configuration changes still run the matrix. Manual dispatch always runs the native suites. This workflow has no signing secrets, does not publish and does not block packaging.

The Windows updater signature is **not** an Authenticode certificate. This release pipeline has no Windows publisher certificate configured, so Windows can show an unknown-publisher/SmartScreen notice. macOS signatures are preserved, but Apple notarization is not configured. Neither limitation prevents cryptographic verification by the Jarvis updater.

## Validate without publishing

In **Actions → Release Desktop → Run workflow**, use `main`, leave the tag empty and disable `publish`, or run:

```sh
gh workflow run release-macos.yml --ref main -f publish=false
```

Installers and their signature checks are produced as workflow artifacts; full quality gates remain local. No tag or public release is created, and clients are not offered an update. Use **Native integrations → Run workflow** to rerun platform integration tests separately.

## Signing setup and recovery

The existing Mac with the original signing identity can configure the protected GitHub environment with:

```sh
bun run release:setup
```

This reads `~/.jarvis/release/updater.key` and verifies its `.pub` against `plugins.updater.pubkey`. Alternatively, `TAURI_SIGNING_PRIVATE_KEY` can point to a key file with its `.pub` beside it. Set `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` for an encrypted key.

Setup exports only the selected Apple identity to a temporary password-protected PKCS#12, sends secrets through GitHub CLI stdin and removes temporary files. macOS may request Keychain access. With multiple identities, select the original fingerprint through `APPLE_SIGNING_IDENTITY`. Setup refuses an environment allowing other branches and preserves existing protection rules.

| Secret in `macos-release` | Purpose |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Existing updater private-key contents, shared by all platforms. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Optional updater key password. |
| `APPLE_CERTIFICATE` | Original Apple identity and private key as base64 PKCS#12. |
| `APPLE_CERTIFICATE_PASSWORD` | PKCS#12 password. |
| `APPLE_SIGNING_IDENTITY` | Original certificate SHA-1 fingerprint. |

GitHub cannot return secret values after upload. Keep secure backups outside Git. Replacing the updater key breaks trust for existing installations; do not generate a new production key for Windows or Linux.

## Failure and retry

A failed local gate leaves the version commit locally but creates no tag and sends nothing to GitHub. Fix and commit any code changes, synchronize main and rerun the release command. If the failure was transient, the unchanged local version commit can be retried directly; the gates run again.

A missing platform or failed installer/signature check prevents publication. Inspect Actions logs, then rerun **all jobs** for transient failures so the publish job receives the complete artifact set for that attempt. If dispatch failed after push, the same launcher command can be repeated while the validated tag points to HEAD; omit the notes file on a retry. If `main` advanced, manually dispatch on `main` with the original validated tag and `publish=true`. Older tags without the local validation declaration can still be compiled with `publish=false`; publish a new version through the launcher instead of rewriting them.

An incomplete draft can be resumed. Code corrections require a new version after publication. To remove a problematic release from update discovery without changing installed apps:

```sh
gh release edit vVERSION --draft
```

Publish fixes with a higher version; the updater does not downgrade.

## Published files and client behavior

Each complete desktop release contains twelve assets:

- macOS DMG, `.app.tar.gz` updater package and `.app.tar.gz.sig`.
- Windows `-setup.exe`, reused by the updater, and `-setup.exe.sig`.
- Linux `.deb`, `.AppImage` updater package and `.AppImage.sig`.
- Combined `latest.json`, `latest-darwin-aarch64.json`, `latest-windows-x86_64.json` and `latest-linux-x86_64.json`.

Each platform manifest contains its own package URL and signature, plus the same version, date and notes. The updater only accepts assets under the corresponding `v<version>` tag in the official repository. Build metadata stays in CI artifacts.

The update UI shows download progress and verifies the package before installation. Agents, compactions, terminals and Core installations must finish first. New executions are blocked during the update and layout preferences are persisted.

Install the first Windows version using NSIS; install the first macOS version by copying the app from the DMG to Applications. Windows uses passive NSIS updates. On macOS, the new process confirms native-window creation with the expected version and a temporary local handshake; if reopening fails, the previous window offers a retry without downloading again. A writable installed app is required; an app running from a mounted DMG cannot update in place.

Publishing does not launch or install the application locally. Unit tests cover channels/platform discovery, signed manifests, complete artifact sets, version/commit mismatches, tampering, launcher guards and update UI. A real installed A-to-B upgrade still requires validation on each destination OS.

On Linux, install the DEB with the distribution package manager, or make the AppImage executable and launch it from a writable location. Both formats require a desktop Secret Service session with an unlocked persistent wallet (GNOME Keyring or a compatible implementation). Development and production use different credential service names. A missing/locked service returns recovery guidance without a plaintext fallback.

Automatic Linux updates require a release AppImage whose file and parent directory are writable. DEB, standalone binaries and development builds offer the downloads page instead. Upgrade DEB using the package manager; Jarvis never overwrites its owned binary with an AppImage. An extracted AppImage is useful for diagnostics but does not qualify normal launch or in-place updating. See [Linux validation](PLAN-VALIDACAO-LINUX.md) for prerequisites and qualification evidence.

## References studied

Codex's `rust-ci.yml` and `rust-release.yml` separate validation from release construction. OpenCode's `test.yml` and `publish.yml` use the same boundary; OMP's `ci.yml` separates generic checks and native work. Jarvis applies that separation with local full gates and scoped native CI while retaining its exact-source checks, signing and atomic desktop publication. All reference trees remain unchanged.
