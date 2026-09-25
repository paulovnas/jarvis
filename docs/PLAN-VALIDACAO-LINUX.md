# Jarvis on Linux — readiness audit and native validation guide

This guide prepares validation on a **real Linux desktop**. The original source
audit was written on macOS on 2026-09-24. Linux implementation and its automated
evidence are recorded below; authenticated desktop acceptance remains separate.

| Item | Baseline |
| --- | --- |
| Application version | 1.4.0 |
| Source inspected | `6aaae74` plus the six harness improvements from `jarvis-i1q3`, included in this delivery |
| Documentation task | `jarvis-bssa` |
| Linux implementation | `jarvis-cpj` (the original `jarvis-n18e` is absent from this local Beads database) |
| Whole-task performance measurements | `jarvis-8s54` |
| Related guide | [Windows audit and migration](PLAN-MIGRACAO-WINDOWS.md) |

Beads owns work status. The scenario IDs below identify reproducible tests, not
a second task tracker. Record results against the exact tested commit; do not
carry forward a pass from macOS, Windows, another distribution or another package.

## Linux implementation update — 2026-09-25

The initial distribution target is **Linux x86_64, DEB and AppImage**. The release
workflow builds on Ubuntu 22.04; native tests in this working tree run on Linux
Mint 22.3 (Ubuntu noble), kernel 6.14, glibc 2.39, GTK 3.24.41 and WebKitGTK 2.52.6,
with Rust 1.98.1 and Bun 1.4.2. These are distinct build/validation environments.
Linux ARM64, RPM, Alpine/musl and NixOS are not qualified by this implementation.

- Providers (Codex, Antigravity and Custom), MCPs and Context7 now share the
  persistent Secret Service backend in `src-tauri/src/secrets_linux.rs`.
  Development/production and provider/MCP service names are separate. Locked or
  absent wallets report recovery instructions; there is no plaintext fallback.
- `keyring` uses `sync-secret-service` and Rust encryption on Linux. This follows
  Codex's native credential lifecycle while fitting Jarvis's blocking storage
  contract, without a kernel-only cache. OpenCode's permission-restricted JSON
  and OMP's permission-restricted SQLite credentials were compared; neither
  replaces Jarvis's requirement for a system credential vault.
- A bounded `/bin/true` preflight now exercises the selected Bubblewrap profile.
  An installed but unusable adapter reports unavailable isolation and uses the
  existing informed approval path. User commands are never replayed by the probe.
- Release staging requires DEB, AppImage and its updater signature. Publication
  requires all three desktop targets and produces `latest-linux-x86_64.json`.
  Automatic Linux updates are restricted to writable release AppImages; DEB and
  other installations offer manual downloads.
- Git/gh installation remains manual on Linux, with the existing official links
  and recheck flow. It does not require adding an automatic privileged installer.

Run the native credential regression suite with:

```bash
bash scripts/check-linux-keyring.sh
cargo test --locked --manifest-path src-tauri/Cargo.toml linux_sandbox_enforces_filesystem_and_network_boundaries -- --ignored
```

The script creates a temporary private D-Bus session with service activation
disabled and a disposable GNOME Keyring data directory. Synthetic keys cover all
consumers, overwrite/delete, profile isolation, service absence, locked access,
and persistence after restarting/unlocking the daemon. It never changes `HOME`
or uses the desktop user's wallet. Install `gnome-keyring` and `dbus-x11` for this
opt-in test. Ordinary `cargo test` does not prompt for or access real credentials.

Implementation Bead `jarvis-cpj` is complete. Native desktop qualification is
tracked separately in **`jarvis-0jb`**. No commit, push, remote workflow or public
release was performed. The L01–L39 desktop/provider/upgrade matrix below still
needs per-environment acceptance evidence; unit tests are not desktop passes.

| Local check on Mint 22.3 | Result |
| --- | --- |
| `bun run check` | PASS: lint, TypeScript, 124 test files / 686 tests passed / 1 skipped, Vite build and generated IPC contracts |
| `cargo clippy --locked --all-targets --features browser-probe -- -D warnings` | PASS, including the native probe |
| `cargo test --locked` | PASS: 817 passed, 23 opt-in tests ignored |
| `cargo fmt --check`, shell syntax and release workflow YAML | PASS |
| Private Secret Service test script | PASS: absent service, all consumers, profile isolation, overwrite/delete, locked read/write/delete, persistence after daemon restart/unlock |
| Opt-in native Bubblewrap boundary test | PASS: project writes and authorized localhost access; external writes and isolated-network access denied |
| Native browser probe (isolated X11/Xvfb) | PASS: exact GTK/DOM viewport matching its host slot even after oversized content loads, main-window resize, no page hide during bounds changes, navigation, fill/click, visible/hidden captures and denied remote IPC |
| Local release binary and `tauri bundle --ci --bundles deb,appimage --no-sign` | PASS: DEB and AppImage generated; no production signing keys used |
| Package inspection | PASS: amd64/version/dependencies, desktop entry validation, no unresolved binary libraries on this host |
| Real provider login, clean-machine install, Wayland/X11 and signed A-to-B upgrade | NOT RUN: `jarvis-0jb` |

The onboarding regression tracked by `jarvis-ufp` was reproduced as GitHub's
anonymous REST quota exhaustion (`403`, `x-ratelimit-remaining: 0`), while npm
and Node downloads returned `200`. All six existing component directories and
required files were present; the managed Node, Bun, Beads and Dolt binaries ran
successfully. Remote update failures now leave installation health intact.
Release metadata is reused for 30 minutes within the process, rate-limit
deadlines are respected across repositories, and completed update attempts do
not repeat when the Core panel remounts. Ponytail source archives use codeload
directly, verified with the official 4.10.0 package. A missing Context7 key
configuration now requests a key instead of reporting a filesystem failure.
Regression tests cover cache expiry, rate-limit recovery, unchanged local
readiness, preserved installation errors, and update events/remounts.

The first agent execution failure (`jarvis-9lh`) was reproduced with a local WSS
handshake: both Rustls crypto providers were enabled, so the incremental
WebSocket client panicked before sending inference. HTTP clients chose their
provider explicitly, and the existing WebSocket tests used plaintext localhost
connections. Application startup now selects AWS-LC for the process. Root runs
also supervise panics, finalize the journal and emit an error instead of leaving
an orphaned active turn. Workflow failures still reach worker shutdown and
terminal state persistence. Regression tests cover TLS handshake failure and
cancellation, preserved partial progress and queued messages after a panic,
and accepting another message without restarting the app. This identifies a
transport initialization defect, not a Linux-specific provider limitation.

The subsequent embedded-browser defect (`jarvis-xy0`) was reproduced in the
native X11 probe: adding a child reduced the main interface's allocation.
Tauri 2.11.5 builds its Linux children in `GtkBox`; Wry's bounds setters do not
position those children (see the [GTK container contract](https://docs.rs/wry/0.55.1/wry/trait.WebViewBuilderExtUnix.html)).
Linux pages now use a GTK overlay with logical position and size, keeping the
main interface independent of page geometry. The first overlay fix still used
a minimum size request: after content loaded, WebKit's natural size could grow
the page over the Inspector and footer. An oversized-page regression reproduced
an allocation of `1000x680` instead of the requested `880x600`. The overlay now
assigns exact child bounds through GTK's `get-child-position` signal, independently
of page content. Viewport updates no longer hide the active page, and navigation
retains the existing viewport. After page load and dynamic content growth, the
probe checks translated GTK coordinates and dimensions, the page's DOM viewport,
its matching host slot, growing/shrinking windows and absence of unmap events.
It also saves `native-browser-host.png` with the surrounding sidebar, Inspector
and footer visible, alongside its navigation/capture/IPC checks.
This is X11 fixture evidence; Wayland and the user's full project flow still
need desktop acceptance. Frontend checks also cover contextual Copy/Paste
(including portal fields and composer undo), unchanged scoped menus, and model
catalog refresh inside the model selector. Passive tab and toolbar tooltips keep
the native page visible on hover; interactive menus and dialogs still occlude it.

Historical local test artifacts (not published; removed by the required
`cargo clean` after native builds exceeded 30 GiB). Rebuild to include
`jarvis-ufp`, `jarvis-9lh` and `jarvis-xy0`; this host's glibc baseline is
**2.39**, not the CI builder's baseline. Previous hashes are retained as evidence:

| Artifact under `src-tauri/target/release/bundle/` | SHA-256 |
| --- | --- |
| `deb/Jarvis_1.4.0_amd64.deb` | `dd502ab6e26701a2e73d920c679d364e4745a701ea079ecceb7fd66b837d57e1` |
| `appimage/Jarvis_1.4.0_amd64.AppImage` | `d4c688950fbea4c51f921701aa6f7c0c342758530313b3033d5e3db1c736ad9c` |

The local packaging check used a temporary Ubuntu `patchelf` binary because the
system package was absent. CI installs it explicitly. Native browser builds
triggered target cleanup at 31 GiB; no app or Rust build process was active
during cleanup.

## 1. Original readiness audit and confirmed gaps (2026-09-24)

Linux shares Unix process and filesystem behavior with macOS, but uses a
different WebView, desktop session, credential service and sandbox. Those
differences are material for this application.

| Priority | Current source evidence | Consequence and required validation |
| --- | --- | --- |
| P0 | [Provider credentials](../src-tauri/src/openai_codex.rs): the non-macOS/non-Windows `SecretStore` returns `Unavailable` for load, store and remove. | Persistent Codex, Antigravity and Custom account credentials are blocked. Implement a Linux backend before considering authenticated chat usable; merely installing GNOME Keyring will not fix missing application code. |
| P0 | [MCP credentials](../src-tauri/src/mcp/mod.rs) have the same unsupported-platform branch. [Context7](../src-tauri/src/core/context7.rs) uses that store. | Validate MCP configuration persistence and Context7 credentials through the Linux backend as well. Do not restrict the fix to provider accounts or silently store secrets in plaintext. |
| P1 | [Release targets](../scripts/release-plan.ts) and [CI](../.github/workflows/release-macos.yml) only build macOS ARM64 and Windows x64. Artifact naming and manifest validation assume those targets. | No Linux delivery pipeline exists. The Ubuntu publish job only uploads other platforms' artifacts. Adding a runner alone is insufficient. |
| P1 | [Sandbox detection](../src-tauri/src/agent/execution_sandbox.rs) finds `bwrap` on PATH, then reports the adapter available. It does not prove that user namespaces can actually be created. | Exercise the real profile, especially on Ubuntu/AppArmor and Fedora/SELinux. Presence of the executable is not proof that a command can run. |
| P1 | [Updater eligibility](../src-tauri/src/updater/mod.rs) accepts a non-debug binary on non-macOS platforms without distinguishing Linux package formats. | Establish AppImage versus DEB/RPM update behavior before distribution. A native package must not be treated as an automatically replaceable AppImage. |
| P2 | [Optional tools](../src-tauri/src/optional_tools.rs) detect Git/gh, but automatic installation only supports Homebrew and WinGet. | Linux currently needs official manual installation instructions. Validate that onboarding reflects this, supports rechecking, and does not advertise a nonexistent automatic installer. |
| Validation required | Core asset selection accepts Linux x64/ARM64; native browser capture has a Linux implementation. | This is partial preparation, not proof that upstream binaries, native modules, WebKitGTK screenshots or the packaged application work on the target machine. |

In the original audit, P0 blocked authenticated operation and P1 blocked a
dependable release. The implementation update above addresses those source gaps.
P2 remains a documented limitation. Native acceptance scenarios start as **NOT
RUN** and require their own evidence, even after the backend is implemented.

## 2. Proposed validation matrix

Start small, then expand based on evidence. These are candidate environments,
not an announcement of support.

| Environment | Purpose | Package/session coverage |
| --- | --- | --- |
| Ubuntu 24.04 LTS, x86_64, GNOME | First development and installed-app baseline | DEB and AppImage; Wayland and a separate Xorg login where available |
| Debian 12, x86_64, desktop session | Older glibc/WebKitGTK compatibility candidate | DEB and AppImage, at least X11 |
| A supported Fedora release, x86_64, KDE or GNOME | Different package manager, desktop integration and SELinux | AppImage; RPM only if chosen for distribution; Wayland |
| Linux ARM64 | Separate later qualification | Build, all downloaded Core binaries, browser and updater must be qualified independently |

Record the exact distribution release, kernel, CPU, GPU/driver, glibc, desktop,
display backend, scaling, WebKitGTK, package format and package hash. Wayland and
X11 must be separate sessions/runs; a Wayland session can also expose `DISPLAY`
through XWayland. Note the effective GTK backend when comparing results.

A container can validate build and CLI tests. Xvfb can support additional smoke
tests. Neither replaces a desktop with user D-Bus, credential service, portals,
notifications and graphics. WSL/WSLg is supplemental evidence. Alpine/musl and
NixOS require separate dependency and path work before any support claim.

Build distributable Linux binaries on the **oldest base system in the chosen
support matrix** that supplies WebKitGTK 4.1. A build made on Ubuntu 24.04 cannot
be assumed to run on Debian 12 or Ubuntu 22.04. AppImage does not remove glibc
compatibility requirements. Also check the separately downloaded Node, Bun,
Beads and Dolt binaries on that oldest system.

## 3. Prepare a disposable desktop account

Use a dedicated Linux account or a VM with a snapshot and a real graphical
session. Keep production projects and credentials out of the fixture. Run Jarvis
as the desktop user, never through `sudo`.

The application currently uses `~/.jarvis` for production and `~/.jarvis-dev`
for development, rather than XDG directories. `bun run tauri dev` selects the
development profile. Packaged release builds use the production profile of the
test account. Do not redefine `HOME` or delete an existing profile to create
isolation: the desktop credential service and single-instance identity must
belong to the same test user as the application.

### Build prerequisites: Ubuntu/Debian candidate

Run the following **on the Linux test machine**, adapting package availability
to its release. This is a development environment, not a list of tools every
Jarvis end user must install.

```bash
sudo apt-get update
sudo apt-get install -y build-essential curl wget file pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev libssl-dev libdbus-1-dev \
  libayatana-appindicator3-dev librsvg2-dev patchelf git
```

Install Bun and Rust using their official instructions. Bun 1.3.14 is the
version currently pinned in release CI; Rust uses stable with Clippy. Record
the actual versions rather than assuming they match another machine.

```bash
bun --version
rustc --version
cargo --version
git --version
pkg-config --modversion gtk+-3.0 webkit2gtk-4.1 openssl
uname -srmo
cat /etc/os-release
getconf GNU_LIBC_VERSION
printf 'Session=%s Desktop=%s\n' "${XDG_SESSION_TYPE:-unknown}" "${XDG_CURRENT_DESKTOP:-unknown}"
```

Additional environment requirements, with separate failure handling:

| Dependency | Purpose and expected behavior |
| --- | --- |
| WebKitGTK/GTK runtime and CA certificates | Application rendering, native browser, HTTPS and OAuth. A clean-machine package must declare or bundle its required runtime libraries. |
| Desktop D-Bus and notifications service | Notification delivery and persistent credentials. A successful permission call alone does not prove that a banner appeared. |
| A compatible persistent Secret Service | Test unlocked, locked, absent and refused access. GNOME Keyring is used by the automated native test; KDE/KeePassXC desktop integration must still be demonstrated with Secret Service enabled and the wallet unlocked. |
| `bubblewrap` | Command sandbox adapter. The kernel and desktop security policy must permit the selected namespaces. Missing or unusable support needs a clear, recoverable path. |
| Git and optional GitHub CLI | Git actions require Git; PR/merge actions require authenticated gh. Missing gh must not prevent local editing or ordinary chat. |
| FUSE for the chosen AppImage runtime | Verify the distribution's matching package (`libfuse2t64` on Ubuntu 24.04 or `libfuse2` on Debian 12, when required). An extraction workaround is a diagnostic, not a passed normal-launch test. |
| Project-specific runtimes | PHP, Python, Java, containers, etc. are project dependencies, not automatically provided by the Jarvis Core. |

Do not preinstall global Node/Bun solely to make a Core smoke test pass. The Core
has managed runtimes, including Node 22.23.2 in this source revision; verify them
independently from the developer's Bun installation. Preserve any missing-runtime
failure before trying a workaround.

## 4. Build and automated gates

Use a checkout containing this delivery, with dependency lockfiles intact. From
the repository root:

```bash
git rev-parse HEAD
git status --short
bun install --frozen-lockfile
bun run check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
bun run eval:harness
```

Keep complete logs and each exit status. `bun run check` includes lint,
typecheck, frontend tests, build and generated IPC checks. Do not replace native
tests with a successful Vite preview. Platform-specific tests mean Linux test
counts may differ from the macOS baseline of 665 frontend and 811 Rust passes.
Ignored live/provider tests require their own deliberate execution and evidence.

If a test fails only under concurrency, preserve the original failure and run
that test alone; an isolated pass does not explain the cause. A serial rerun can
help diagnosis but must be labelled. Do not hide warnings or update unrelated
snapshots to obtain a green run.

Measure `src-tauri/target` and disk space before large builds. Follow the repository
30 GiB / 15% free-space cleanup rule, and never clean while a running Jarvis,
Cargo or rustc uses the target directory.

```bash
du -sh src-tauri/target
df -h .
bun run tauri dev
```

After development smoke tests, stop that instance before testing a package in the
test account. Build native test artifacts without the production updater-signing
configuration:

```bash
bun run tauri build --bundles deb,appimage -- --locked
```

This command builds local Linux candidates without production signatures; it does
not publish or qualify a distribution. Inspect actual outputs under
`src-tauri/target/release/bundle/`, including package dependencies, architecture,
desktop entry, icon and executable permissions. Keep SHA-256 hashes. Test both
launcher-menu and terminal startup. For AppImage, record its original file path
and whether that path is writable by the desktop user.

## 5. Test fixture and evidence rules

Create one small Git repository and one project directory containing **separate**
`Frontend`, `Backend` and optional `Microserviço` repositories. The parent should
contain documentation but no `.git`. Use spaces and accented names, plus two
case-distinct files such as `Example.ts` and `example.ts` on a case-sensitive
filesystem. Keep a second unrelated directory as an authorization boundary.

Use local bare remotes for commit/push tests. PR/merge tests need a disposable
GitHub repository and explicit test authorization. Use a disposable PostgreSQL
database for migration checks. No scenario authorizes operations on production.

Capture expected and actual results, elapsed time, sanitized diagnostic export,
screenshots where relevant, and resulting files/commits/database rows. Do not
attach raw environment dumps, OAuth callback URLs, tokens, MCP headers, credential
blobs or real conversation journals. Review screenshots and diagnostics before
sharing them. Preserve original failure evidence before repair or reinstallation.

## 6. Native acceptance scenarios

Every row is NOT RUN until recorded on the named Linux environment. Test success means the
observable result below, not merely that the agent claimed completion.

### Application, credentials and desktop

| ID | Procedure | Required observable result |
| --- | --- | --- |
| L01 | Start from the desktop launcher with a clean profile, then reopen with an existing profile. | Bootstrap progresses or explains a failure; window becomes visible without a blank WebView. Data and project selection survive reopening. |
| L02 | Launch a second instance of the same profile; separately exercise development versus packaged identity. | No concurrent writer for one profile; existing instance receives focus. Development and production data do not mix. |
| L03 | Finish onboarding with Git/gh absent, then install them externally and recheck. | Linux shows actionable manual instructions; optional dependencies can be skipped; refreshed detection and Git/gh authentication status are accurate. |
| L04 | Once implemented, connect Codex and Antigravity, restart, refresh tokens/models, disconnect and reconnect. | Account persists securely, aliases stay distinct, refresh works without unnecessary login, removal deletes the matching credential. No secret enters SQLite/config/log output. |
| L05 | Configure Custom Responses, Chat Completions and Messages endpoints; add two accounts with the same model name. | Correct endpoint/protocol/account is used, provider identity appears in the composer, model refresh/toggles behave correctly, and removed models can be remapped. |
| L06 | Repeat credential operations with the service locked, absent and unlock denied. | Clear recovery without data loss, endless wait, fake successful login or silent plaintext fallback. A subsequent unlock can recover. |
| L07 | Configure authenticated stdio/HTTP MCPs and a Context7 key; restart and remove them. | Each consumer uses the Linux credential backend, preserves its configuration correctly and cleans up only its own secret. |
| L08 | Repeat window resizing, dialogs, copy/paste and file/folder pickers in Wayland and X11, including 125%/200% scaling. | Usable decorations, focus, positioning, scrolling and HiDPI rendering; paths and selection are intact. No required UI is offscreen. |
| L09 | Send a limit notification and a completed-task notification; switch focus and return to the chat. | Desktop delivery works; unread state clears; a reached limit alerts once per intended session/window policy, not on every refresh. DND suppression is distinguished from application failure. |
| L10 | Suspend/resume and temporarily disconnect networking during an active read-only task. | UI remains responsive, reconnect/retry is explained, confirmed results survive and uncertain effects are not blindly replayed. |

### Core, MCPs and skills

Test each Core component through **install, health check, use, update and repair**.
Successful installation alone is insufficient.

| ID | Component/scenario | Required observable result |
| --- | --- | --- |
| L11 | Beads plus managed Dolt; planned flow, comments and blocked epic recovery. | Correct Linux binaries start; plan/task state is scoped to the chat; comments are visible at implementation boundaries; manual closure behaves correctly without corrupting external project Beads. |
| L12 | Context-mode with a large synthetic tool result, compaction and restart. | Managed Node/Bun and SQLite FTS5 work; native capture/retrieval and hook receipts are present; retained context is bounded and recoverable. |
| L13 | Open Design with the native Designer; update with unknown download size, then repair an intentionally damaged test installation. | Design assets are actually consulted; progress remains visible; repair is available; failure retains or restores a usable installation. This resource integration must not be mistaken for launching every upstream daemon. |
| L14 | Ponytail through a coding/design task. | The installed guidance is consumed by the runtime for the relevant task and remains subordinate to user/project instructions; no mandatory ceremonial tool loop. |
| L15 | Managed TypeScript LSP in the fixture repository. | Definitions, references, symbols and diagnostics work without a globally installed `typescript-language-server`; processes close cleanly. |
| L16 | Context7 lookup after credential support is available. | Managed runtime starts, the selected documentation lookup returns usable evidence and failure does not silently switch integrations. |
| L17 | Explicit MCP request, large catalog search, injected timeout and one invalid argument. | Scope survives recovery/compaction/handoff; focused search exposes matching schemas on the next step; validation identifies the bad argument; no unrelated MCP or blind replay. |
| L18 | Marketplace install/details/update; a large monorepo and repeated skill names; toggle during update checks. | Correct skill subdirectory is selected; details/loading recover; enabling remains usable; size/file limits apply to the intended installation scope. |
| L19 | Edit, add, disable and remove a local skill between rounds; switch projects. | Cached catalogs invalidate; disabled skills cannot be read through a stale cache; unchanged rounds reuse the immutable catalog. |
| L20 | Interrupt a Core download and restart offline; retry online. | Staging is recoverable, diagnostics explain what failed, optional checks can be skipped where supported, and skill-update checks do not hold bootstrap hostage. |

For L11–L16, record component version, managed runtime version, architecture,
installation path and health/use evidence. Verify executable bits and native
module ABI on the actual distribution. Test a restricted/noexec download or
installation location as a negative case; do not prescribe a global mount-policy
change as the application's remedy.

### Commands, terminal, browser and Git

| ID | Procedure | Required observable result |
| --- | --- | --- |
| L21 | Open terminals with automatic shell, then explicit bash/zsh/fish where installed; change font. | Login-shell selection and configured arguments are correct; glyphs/font changes, resize and Unicode paste work. Agent shell commands still use their documented bash contract. |
| L22 | Start a fixture dev server and a command with child processes; interrupt/cancel/close. | Output and exit state are correct; children terminate and the port is released. Agent-owned terminals can close under current rules; pre-existing terminals use the required approval. Sidebar terminal indicators stay accurate. |
| L23 | Launch from the menu with a reduced PATH; use a stdio MCP installed under a supported version manager. | Runtime/shebang resolution works or gives an actionable error; no reliance on opening Jarvis from a specially prepared terminal. |
| L24 | Use sandboxed commands to write inside the project, attempt an unauthorized external write, access an authorized localhost server/database and run an isolated-network command. | Actual write/network behavior matches the displayed policy. Necessary approved localhost work succeeds; denied access yields recoverable guidance without repeating uncertain effects. |
| L25 | Repeat with bwrap missing, and with bwrap present but user namespaces blocked. | Absence and runtime failure remain distinguishable. Required informed approval/escalation is usable; the agent can continue after authorization rather than entering a refusal loop. |
| L26 | Ctrl-click a terminal localhost URL; browse that page, capture it, inspect DOM/console and change tabs/chat. | Links open once through the intended route; WebKitGTK capture returns a valid image; browser state is scoped correctly and an external page cannot invoke main-window Tauri commands. |
| L27 | Apply a transactional patch to the nested Backend repository; edit case-distinct and accented filenames; test an escaping symlink. | Valid nested paths work, outside-root access follows authorization, and failure is atomic. Linux case sensitivity does not merge distinct files. |
| L28 | Publish fixture changes in the nested repositories; reuse an existing PR and merge under authorization; approve with an observation. | Correct repository/branch/remote is used; the observation is considered first; existing PR is reused; approval and questions reach the subagent card; changed-files state clears after verification. |

Install `bubblewrap` using the distribution's package manager in the test
environment. This minimal launch probe checks namespace availability without
touching a project:

```bash
command -v bwrap
bwrap --die-with-parent --new-session --unshare-user --unshare-pid \
  --unshare-ipc --ro-bind / / --dev /dev --proc /proc --tmpfs /tmp \
  --cap-drop ALL --unshare-net -- /bin/true
```

Still execute L24 through Jarvis: this probe does not test its working-directory
binds, permission decisions or grant reuse. Inspect AppArmor/SELinux and namespace
errors before changing the application. Do not disable host security globally to
declare success. The current Jarvis profile exposes `/` read-only and overlays
one writable working directory; it is **not** a confidentiality boundary hiding
all other files, nor the full Codex seccomp/Landlock/metadata-protection stack.

An additional local browser probe is available on a real graphical session:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml \
  --features browser-probe --example browser-probe
```

It opens test windows and writes artifacts under `.codex/browser-review`. Use
the disposable account; a successful probe supplements L26 rather than replacing
the packaged UI test. No global Chrome/Playwright installation is needed for the
integrated WebKitGTK browser.

### Chat continuity and the new efficiency work

| ID | Procedure | Required observable result |
| --- | --- | --- |
| L29 | Send rapidly, switch chats/windows during streaming, then reload/reopen; repeat for all three Custom protocols and connected providers. | User messages and responses never disappear; order, tasks, subagents and selected chat stay correct without navigating away to repair the display. |
| L30 | Run a long planned task and a direct task; answer ask_user, queue/reorder/edit/send-now messages and use manual validation on/off. | Activity/timer stays above the composer while active; observations remain readable above collapsed actions; answered questions join history; auxiliary messages preserve the ongoing objective. |
| L31 | Inject a journal delay/failure in an isolated test fixture; cancel, restart and retry from a durable result. | Other work remains responsive; failed durability blocks dependent effects; acknowledged messages/results/approvals survive; retry does not duplicate confirmed actions. Never corrupt a production journal to test this. |
| L32 | Use 128+ actions, compact and continue with explicit MCP/user constraints. | The 80% automatic threshold is preserved; summary facts, authorization, intent and receipts survive; a fitting summary uses one request; actual overflow shrinks the portion without dropping history. |
| L33 | Delay the end of a Responses stream containing complete independent reads, then disconnect/retry; also test a mutation before a read. | At most four eligible native reads begin early, with ordered durable envelopes/results; partial JSON and mutations do not execute early. Messages, Chat Completions and Antigravity retain terminal-response fallback. |
| L34 | Export/import settings into a fresh test profile and remap models; copy a code block/final response and save Markdown. | Intended settings, agents, flows, repositories and personalization survive; excluded provider credentials stay excluded; copy/save produces exact text and cancellation changes nothing. |

Use [the paired evaluation guide](evaluations/efficiency-2026-09-24.md) for latency,
tokens/cache, errors, recovery and phase timing. Keep the same model, effort,
account capabilities, fixture state and cache condition for comparisons. Include
functional artifacts and sample counts; overlapping durations are not additive
wall time. A fast response with the wrong artifact fails acceptance.

### Packaging, updating and lifecycle

| ID | Procedure | Required observable result |
| --- | --- | --- |
| L35 | Install each candidate on a clean machine without the development toolchain; start through its launcher and file path. | No undeclared shared libraries, missing executable permissions, broken icon/category or dependency on checkout paths. Confirm the exact architecture and oldest supported glibc. |
| L36 | Reinstall/upgrade and remove the package; reinstall again. | Application files follow package-manager ownership; user settings/history remain under the documented policy; launcher entries are not duplicated. |
| L37 | Once Linux artifacts exist, update an AppImage from version N to N+1; repeat with invalid signature, interrupted download and unwritable location. | Correct `latest-linux-x86_64.json` (or separately qualified ARM64 manifest), `.AppImage` and `.sig`; clear progress/error; no unverified installation; previous usable version/data survive failure. |
| L38 | Update while idle, then attempt during active agents, Core installation and open processes; restart twice. | Busy conditions are accurate; accepted restart drains state, releases the single-instance lease and opens exactly one successor at the new version. |
| L39 | Check updates from DEB/RPM installations. | The chosen package-manager/manual-update policy is explicit. Do not report in-app update support merely because the release binary passes `installable()`. |

The Jarvis updater artifact for Linux is the AppImage itself plus its signature.
The release scripts now stage Linux installers and create Linux manifests as
part of the complete desktop release. Signed update tests must use a
controlled test channel/key and an explicitly authorized destination, without
publishing test packages to the production channel or committing private keys.

## 7. Implementation and qualification order

1. Establish the Linux build and UI baseline. Record errors before installing
   unrelated dependencies or applying environment-variable workarounds.
2. Implement one secure Linux credential backend serving providers, MCPs and
   Context7, preserving development/production namespace isolation. Add unit
   tests plus an opt-in native read/write/delete/locked-service test.
3. Validate managed Core runtimes and native command/process behavior, including
   absent or unusable bwrap and authorized localhost access.
4. Run authenticated direct/planned flows, recovery, browser and desktop cases
   in the baseline session; repeat affected cases under the other display backend.
5. Choose package/update policy, then implement native CI, artifacts and manifests.
   Qualify clean installs and signed upgrades on every claimed target.
6. Complete the remaining desktop/distribution matrix and paired performance
   measurements. Track defects as follow-ups to `jarvis-cpj`; keep a failed or blocked cell
   visible until its exact acceptance test passes.

The source changes described above implement the credential, packaging and
preflight steps. Provider sessions, installation, live upgrades and the full
desktop matrix still require native acceptance; no paid sessions were started.

## 8. Evidence record and release gate

Use one record per scenario/environment. Attach it to the relevant Bead:

```text
Scenario ID:
Commit / app version / package SHA-256:
Distribution / kernel / architecture / glibc:
Desktop / Wayland or X11 / GTK backend / GPU / scaling / WebKitGTK:
Package format / startup route / runtime profile:
Core versions / provider protocol and model (no credentials):
Fixture and initial state / warm or cold cache:
Exact reproduction steps:
Expected result:
Observed result and elapsed time:
Outcome: PASS | FAIL | BLOCKED | NOT RUN
Sanitized evidence locations:
Defect Bead / limitation / rerun evidence:
```

Release qualification requires all P0/P1 gaps resolved, automated gates green
on Linux, native credential persistence and representative agent flows working,
clean installation and the chosen update path verified, and desktop evidence for
every advertised session/distribution. Any exclusions must be visible in support
documentation. Compilation, unit tests, CI artifact creation, installation,
upgrade and user acceptance are separate results.

**Original audit evidence:** application/reference sources and official Tauri
prerequisite/AppImage/updater documentation inspected. See the implementation
update for Linux execution evidence. Existing macOS harness results are documented
separately and do not qualify Linux.

## 9. Source and reference map

| Concern | Jarvis source / reference |
| --- | --- |
| Build and runtime dependencies | [Cargo.toml](../src-tauri/Cargo.toml), [Tauri configuration](../src-tauri/tauri.conf.json), [launcher](../scripts/tauri.ts) |
| Data profiles, leases and recovery | [data_dir](../src-tauri/src/data_dir.rs), [session writer](../src-tauri/src/agent/session_writer.rs), [journal](../src-tauri/src/agent/journal.rs) |
| Credentials | [providers](../src-tauri/src/openai_codex.rs), [MCPs](../src-tauri/src/mcp/mod.rs), [Context7](../src-tauri/src/core/context7.rs) |
| Core preparation/installation | [installer](../src-tauri/src/core/install.rs), [health](../src-tauri/src/core/health.rs), [native runtime integration](../src-tauri/src/agent/core_runtime.rs) |
| Commands and desktop integration | [shell](../src-tauri/src/agent/shell.rs), [sandbox](../src-tauri/src/agent/execution_sandbox.rs), [MCP PATH](../src-tauri/src/mcp/executable.rs), [notifications](../src-tauri/src/system/notifications.rs), [browser capture](../src-tauri/src/agent/browser/capture.rs) |
| Packaging and updates | [release plan](../scripts/release-plan.ts), [artifact verification](../scripts/release-artifacts.ts), [workflow](../.github/workflows/release-macos.yml), [updater](../src-tauri/src/updater/mod.rs), [relaunch](../src-tauri/src/updater/relaunch.rs) |
| Prior acceptance structure | [Windows guide](PLAN-MIGRACAO-WINDOWS.md), [efficiency evaluation](evaluations/efficiency-2026-09-24.md) |

The local reference trees remain read-only:

- **Codex:** `docs/codex/codex-rs/keyring-store/{Cargo.toml,src/lib.rs}` separates
  platform credentials behind a load/save/delete contract; its Linux feature is
  `linux-native-async-persistent`. Use its error/lifecycle concepts, then verify
  the backend against Jarvis's desktop and credential consumers.
- **Codex:** `docs/codex/codex-rs/linux-sandbox/src/bwrap.rs` composes filesystem,
  namespace and additional policy layers. Its implementation is broader than
  Jarvis's adapter; do not claim equivalent isolation from matching flags alone.
- **OpenCode:** `docs/opencode/packages/desktop/resources/linux/opencode-desktop.desktop`
  and its package scripts provide launcher/packaging reference points. This
  checkout's desktop uses Electron, so its rendering/update behavior does not
  establish Tauri/WebKitGTK compatibility.
- **OMP:** `docs/omp/crates/pi-natives/src/desktop/linux/mod.rs` selects Wayland
  and X11 backends separately. This reinforces the need for separate native
  tests; Jarvis's embedded browser capture is not whole-desktop capture.

Official documentation consulted on 2026-09-24:
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/),
[AppImage distribution](https://v2.tauri.app/distribute/appimage/),
[Tauri updater](https://v2.tauri.app/plugin/updater/).
Toolchain installation references:
[Bun installation](https://bun.sh/docs/installation) and
[Rust installation](https://www.rust-lang.org/tools/install).
