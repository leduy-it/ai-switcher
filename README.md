# Michael Le Profiles

A native macOS app by **Michael Le (duyle)** to manage and switch between **multiple accounts** for AI coding tools — **Claude Code**, **Codex**, and **Antigravity IDE** — from one place.

Built with [Tauri](https://tauri.app) (Rust + React).

This fork is maintained by [Michael Le](https://duyle.me). It is based on the original project by
[Hoàng Phan](https://github.com/hoangpm96/ai-switcher); thank you for the original work.

> ⚠️ Using multiple subscription accounts may violate a provider's terms of service. This app only manages logins locally on your machine — use at your own discretion.

## Requirements and migration

The repository maintains a [living requirements reference](docs/requirements.md) and an
[indexed task status](TASKS.md). Relevant feature and behavior changes update these documents in
the same change, following [the agent instructions](AGENTS.md).

To assess or implement support in another app, use the
[migration audit prompt](docs/prompts/migration-audit.md) and
[migration implementation prompt](docs/prompts/migration-implementation.md). Both follow every
requirement and baseline ID in the current specification.

## ⬇️ Download

Get the latest **`.dmg`** from the [**Releases**](https://github.com/leduy-it/ai-switcher/releases/latest) page — download the `.dmg` under **Assets**, open it, and drag **Michael Le Profiles** to Applications.

> First launch only: the app is unsigned, so right-click it → **Open** (or run `xattr -cr "/Applications/Michael Le Profiles.app"`). See [Install](#install) below.

## Features

- **One window for every tool.** Log in, switch, rename, and remove accounts per tool.
- **Menu-bar quota table.** Click the icon to open a glass table below it: all visible accounts, email, plan, 5-hour and weekly usage. Filter by provider or email, expand each account for reset times, command, read errors and refresh/switch actions. The footer opens the full app or enters full screen. Right-click retains the native quick-switch menu. The top sidebar also opens this dropdown.
- **Minimizable floating overlay.** Pin quota above your other apps from the dropdown or Settings. Its minus button collapses to a small draggable logo bubble; click the logo to expand. Account rows include email and expandable details.
- **Credential backups.** Export one provider from its account tab, or all providers from Settings. Private JSON files contain raw credentials (including original Codex `auth.json`), parsed fields, available email, account settings, quota and usage at export time. Hidden accounts are included. Exports contain plaintext tokens/API keys and should be stored privately; missing sources are explicitly noted.
- **Quota at a glance.** Reads 5-hour / weekly usage for Claude & Codex and per-model quota for Antigravity, shows your **subscription plan** (Plus / Pro / Max) when the API reports it, and exposes Codex usage-limit reset credits in a per-account modal.
- **Blue Liquid Glass identity.** A Michael Le Profiles logo, native macOS Liquid Glass window on macOS 26+, and a direct link to [duyle.me](https://duyle.me).
- **Per-tool auto-switch.** Configure separately for Claude and Codex — the bare command falls back to another account when the active one nears its quota.
- **Usage & cost tab.** Token usage and estimated cost per tool, plus an aggregated **All** view across tools, charted over a selectable date range.
- **Local API gateway.** Expose Claude/Codex subscription accounts through a local OpenAI/Anthropic-compatible server with API keys, model combos, fallback rotation, cooldown handling, and gateway usage tracking.
- **On-demand session prime.** A **Prime ngay** button on each Claude/Codex account opens a fresh 5-hour window when you want one, using the account's existing token over plain HTTP — it never spawns a CLI or rotates a token, so it can't disturb a running session.

### Claude Code & Codex (CLI)

- Each account logs into its own isolated config dir and gets a **dedicated command** (`claude-<name>`, `codex-<name>`) so you can run several accounts in parallel across terminals.
- Codex accounts can also be added from an existing OAuth `auth.json`: choose **Paste JSON** and **Parse JSON**, or **Import file**. Parsing checks the local format and required token fields, then previews the email and duplicate status without creating an account or contacting the provider. Import revalidates the source, copies it into a private per-account profile and refreshes its live quota. The selected source file is left untouched; pasted JSON stays in the open dialog until import.
- The bare `claude` / `codex` command **follows the account you select** (via a shell hook + an "active profile" file). Run `aisw` in an already-open terminal to sync it to the latest selection.
- Chat sessions are **shared across accounts** in the same project, so you can resume work regardless of which account created it. Codex OAuth profiles keep their own `CODEX_HOME` and `auth.json`, while their launchers use the documented [`CODEX_SQLITE_HOME`](https://learn.chatgpt.com/docs/config-file/environment-variables) override to share the thread catalog with Codex Desktop. Session rollouts, archived rollouts, prompt history, and `session_index.jsonl` are linked to the default Codex home; existing index rows are merged before linking. API/proxy profiles remain isolated. A user-defined `sqlite_home` setting takes precedence over the environment variable and should point every profile at the same state directory.
- The Codex shell hook also exports `CODEX_SQLITE_HOME` for subscription profiles, so legacy `CODEX_HOME` aliases inherit the shared session catalog. API/proxy launchers explicitly clear that override.
- API/proxy accounts can point Claude Code or Codex at an external gateway, with one pinned model per generated launcher.

### Desktop account switching and recovery

Version 0.14.0 adds a separate **Codex desktop account** section in the Codex tab and the quota
dropdown. Choose **Codex.app** or **ChatGPT.app**, then enable **Apply desktop when selecting an
account**. Selecting an account refreshes its live quota immediately and queues a desktop handoff.
**Apply desktop** also applies the current CLI selection. These two installed brands share a vendor
identifier and desktop data folder; the chosen brand runs one at a time, preserving the existing UI
data. The account's `auth.json` remains in its profile and is never copied into another credential
store. The main `.codex` catalog and project/sidebar state are shared for local history.

- **CLI selected** and **Desktop backend confirmed** are separate. A desktop launch is confirmed by
  its execution profile plus the running local backend's `account/read` email/workspace identity.
  An `auth.json` file alone does not prove desktop or cloud sign-in. Cloud conversations remain
  subject to the selected account's access; Switcher never tries to resume inaccessible cloud work.
- The default handoff waits for active turns to finish. It requests normal application quit and
  starts the chosen desktop with explicit profile/catalog paths so shell initialization cannot
  replace the chosen profile. A destination backend with an old catalog is repaired only when idle.
  Desktop instances launched outside Switcher with an unknown execution profile wait until the
  user finishes work and closes them; this prevents an uninformed restart of existing sessions.
- **Switch now & recover** saves a durable private checkpoint before requesting interruption of
  eligible local desktop turns. It stores the operation/app/host, thread and turn IDs, cwd/worktree,
  old/new profile, model/execution policy, terminal metadata and last recorded session results.
  Checkpoints live in the app data directory's `desktop-recovery/` with directory mode `0700` and
  file mode `0600`. They contain private session history and should not be published.
- After verifying the new identity, recovery uses `thread/resume` and `turn/start` in the original
  thread. The continuation asks the agent to inspect files/processes, find the last finished step,
  and complete the remaining work without repeating successful or uncertain external actions.
  Persisted send intent, message IDs, turn acknowledgement and history markers prevent blind resend
  after a connection loss. Completed, user-resumed/stopped, approval-blocked, ephemeral, remote or
  unsupported client-tool sessions are left for the original client. RAM/PTY state is not restored.
- The UI shows waiting, checkpointing, switching, restoring, acknowledged continuation and errors
  per session, with cancel, original-session and safe retry actions. **Continued** means a new turn
  was acknowledged; it does not mean that the resumed task completed successfully.
- Credential changes from desktop or CLI trigger an account-specific quota/email refresh while
  Switcher is running. Main, dropdown and overlay receive the same updated snapshot. A failed
  Codex live read stays visible; shared session logs are never substituted as another account's quota.
- **Recover missing local sessions** scans known OAuth profiles, including hidden originals, and
  adds absent local thread catalog entries and complete absent paginated histories to the main home.
  The handoff also runs this repair before reopening a desktop. SQLite creates consistent snapshots
  including committed WAL data; backups live under the main home's `backups/session-catalog-migration-*`
  with private permissions. Existing records/cursors are never replaced. Partial history conflicts,
  unavailable rollout files, active source profiles and unknown schemas are reported. Creator/source,
  model/policy, project/section metadata, attachments and tools are preserved for imported records.
  Auth, enrollment and daemon tables are never imported. Keep these private backups off GitHub.
  A trailing metadata event may leave an existing cursor behind without missing messages; repair
  reports that separately and leaves the cursor for the native backend to replay. Socket symlinks
  are resolved to avoid macOS's path-length limit on managed profile directories.

The installed versions' local Unix-socket account/session reads were inspected without changing
credentials or interrupting work. Builds confirm compilation; switching/recovery still depends on
the native app/backend version exposing the required capabilities and on an accessible local session.
API/proxy profiles continue to use their CLI launchers.

### Local API Gateway

- Start a local server on `127.0.0.1:8783` by default and call it with OpenAI or Anthropic-compatible clients.
- Create local gateway API keys, enable the subscription accounts that may serve requests, and define named **combos** that resolve to ordered model fallbacks.
- Create virtual Claude/Codex CLI accounts that point at the local gateway and pin a combo or single discovered model.
- Gateway usage is tracked separately by combo, key, account, and tool.

### Session prime

The legacy daemon/wake scheduler was removed in 0.7.0 because its token refresh paths could disrupt
running CLI logins. Version 0.13.0 adds app-local automatic prime through the same read-only-token
HTTP path as manual prime. The app must be running and the Mac awake.

- **Automatic Hello.** Enable it in Settings and choose visible Claude/Codex subscription accounts.
  Pro and Team/Business plans use the same eligibility rules. The app checks every minute using
  refreshed quota, validates the live five-hour state before sending, and sends one minimal Hello
  only when a new window can be opened. It skips unknown/failed quota reads, hidden/API accounts and
  weekly locks. Each attempt is recorded before sending and gets a durable five-hour cooldown,
  including unconfirmed sends, so restarts cannot cause repeated greetings. Pause it in Settings.
  It consumes a small amount of quota; it does not refill quota, reset the weekly limit, rotate a
  token, launch a CLI, or install a wake daemon.
  Codex Team/Business/Pro accounts that successfully report weekly quota without a five-hour bucket
  receive a greeting at most once per five-hour cooldown; that result says Hello was sent rather than
  claiming the provider opened a window it does not report.

- **Bắt đầu phiên 5 giờ (on demand).** When the provider reports that an account has no active five-hour window, the card shows a manual prime button. Clicking it sends one minimal request directly over HTTP using the account's existing OAuth token — it never starts the Claude/Codex agent runtime, never spawns a CLI, and never rotates a token, so it cannot trigger macOS protected-folder prompts or disturb a running session. It runs a single attempt (send once, then confirm briefly); if confirmation doesn't land in time, press it again. The UI first says the request was sent and is awaiting confirmation, and reports an opened session only after the reset state is verified.
- Quota for every account (including the machine default) is read live from the provider, so the displayed usage and reset time stay current and a refresh always reflects the real state.
- **Codex usage-limit reset credits.** Codex account cards show a small reset-credit icon in the bottom action row when the provider reports reset-credit data. Click it to see how many resets remain and, when available, each reset credit's expiry time. Accounts with no available credits still show `0 available`; API/proxy accounts do not expose this provider quota data.
- **Upgrading from an older version?** If you had the scheduled-prime wake daemons installed, open **Settings** — when leftover daemons are detected you'll see a one-tap **Gỡ daemon cũ** button that removes them (one admin prompt). The app also clears any stale wake schedule on first launch.

#### Prime and quota troubleshooting

- **Claude quota returns HTTP 401 or 403:** the stored OAuth token has expired. The app does **not** refresh it — refreshing (even via the `claude` CLI) can evict a live terminal session. Open that account's `claude` CLI and log in again; the app's **Làm mới token** button only re-reads the token (it clears a transient 401 if the token is actually still valid).
- **Prime reports a send error:** the prime sends directly over HTTP for Claude/Codex. If a send fails, confirm the profile is logged in and that quota can be refreshed.
- **Codex prime takes a little while to confirm:** sending the prime anchors the Codex five-hour window, but the freshly anchored reset reads close to "now + 5h" for the first minute or so. The app confirms by watching the reset settle to a fixed value (typically within 15–30 seconds) rather than assuming success, so a manual prime may show "awaiting confirmation" briefly before reporting the opened session. If it runs out of time, it says the request was sent but not yet confirmed — wait a moment, refresh quota, and prime again if needed.
- **No Prime ngay button:** the button appears only when quota was read successfully and the provider reports no active five-hour window. Authentication, network, or unknown quota state fails closed rather than offering a prime the app cannot safely verify.

### Antigravity IDE (GUI)

- Switching swaps the saved login token in the IDE's `state.vscdb`. The app quits and reopens the IDE around each swap so the token isn't clobbered.
- **Sign in new account** puts the IDE at a logged-out screen so you can add an account that was never signed in.
- Accounts are identified by their **Google avatar**, and duplicates of the same account are detected automatically.

## Install

1. Download the latest `.dmg` from the [Releases](https://github.com/leduy-it/ai-switcher/releases/latest) page.
2. Open the `.dmg` and drag **Michael Le Profiles** to Applications.

The app is **not code-signed** (no paid Apple Developer account), so macOS Gatekeeper will warn on first launch. To open it:

- **Right-click** the app in Applications → **Open** → **Open** in the dialog, **or**
- Run once in Terminal:

  ```bash
  xattr -cr "/Applications/Michael Le Profiles.app"
  ```

You only need to do this the first time.

## Build from source

Prerequisites: [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 20+, and the Tauri macOS prerequisites (Xcode Command Line Tools).

```bash
npm install
npm run tauri dev      # run in development
npm run tauri build    # produce a .dmg in src-tauri/target/release/bundle/dmg
```

## Releasing

Pushing a version tag like `v0.6.3` triggers the GitHub Actions workflow (`.github/workflows/release.yml`), which builds a universal macOS `.dmg` and publishes a GitHub Release with the artifact attached. Bump the version in `package.json`, `package-lock.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock` first, then:

```bash
git tag v0.6.3
git push origin main v0.6.3
```

See [CHANGELOG.md](CHANGELOG.md) for the per-version history and
[the v0.7.2 release notes](docs/releases/v0.7.2.md) for the current release.

## License

No license file yet — add one (e.g. MIT) before sharing widely if you want to allow reuse.

## Attribution

Michael Le maintains this fork and its visual identity. The underlying project was created by
[Hoàng Phan](https://github.com/hoangpm96/ai-switcher), whose work is gratefully acknowledged.

## Upgrade compatibility

The app retains its existing local profile-data and macOS Keychain service identifiers so saved
accounts and credentials remain available after the rebrand. Those identifiers are internal
compatibility details; the app is presented and maintained as Michael Le Profiles.
