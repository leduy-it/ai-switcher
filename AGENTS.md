# AI Account Switcher Project Memory

## Living requirements and task tracking

- Read [docs/requirements.md](docs/requirements.md) and [TASKS.md](TASKS.md) before feature work.
  The requirements document is the authoritative specification; TASKS.md records current progress.
- For every relevant feature, behavior, provider, UI, credential/session storage, migration or
  release change, update affected requirements and acceptance checks in the same change as the
  implementation. Update the matching task status and material limitations, plus README/changelog
  and docs/prompts when their statements or coverage change.
- Preserve requirement/task indices. Append new indices for new requests; retain superseded or
  retired entries and record the latest user decision. Migration audits must cover every current
  requirement and baseline ID rather than a frozen list.
- Separate source implementation, validation performed, installed-app version and published
  release. Do not claim a feature is validated merely because code exists or compilation succeeds.
- Review the documents against final behavior before reporting completion. If behavior and
  requirements did not change, preserve the specification and update only relevant progress/evidence.
- Keep this reference in the repository. Personal notes, Pages and exports are snapshots; do not
  treat them as the maintained source. Never add tokens, raw credential exports, private session
  contents or machine-specific recovery manifests to public documentation.

## Shape

- Desktop app: Tauri 2 + React/Vite.
- Frontend lives in `src/`; backend commands and account logic live in `src-tauri/src/`.
- App state is persisted by `src-tauri/src/store.rs` under the OS app-data dir from:
  `ProjectDirs::from("dev", "hoangphan", "AI Account Switcher")`.
- Local app-data layout:
  - `state.json`: accepted disclaimer, accounts, auto-switch settings.
  - `accounts/<tool>/<account-id>/`: per-account profile dirs.
  - `active/<tool>.profile`: selected profile path for the bare CLI command.
  - `usage.json` and `litellm_prices.json`: token/cost caches.

## Core Backend Files

- `models.rs`: shared DTOs/enums for Tauri commands and React types.
- `store.rs`: persisted state paths and helpers.
- `tools.rs`: CLI detection, profile login, symlinks, launchers, shell hook, delete cleanup.
- `app_state.rs`: high-level account workflows: snapshot, add, switch, delete, refresh, auto-switch.
- `quota.rs`: quota readers for Claude, Codex, Cursor CLI, opencode Zen, Antigravity.
- `overlay.rs`: the always-on-top quota overlay window (second frameless window, label `overlay`).
- `menubar.rs`: a separate quota-table dropdown anchored to the status icon (label `menubar`).
- `credential_export.rs`: explicit provider/all credential backups; secrets stay in Rust and the
  JSON is saved atomically with owner-only permissions, including raw auth sources and usage.
- `usage.rs`: scans Claude/Codex JSONL logs and builds the Usage tab report.
- `pricing.rs`: LiteLLM price cache and model-price lookup.

## Account Model

- Claude/Codex machine default accounts point at `~/.claude` / `~/.codex`.
- Cursor CLI and opencode are managed the same way, with their own isolation mechanism:
  - opencode: profile dir is an `XDG_DATA_HOME`; credentials land in `<profile>/opencode/auth.json`.
  - Cursor: no config-dir variable exists. Login runs with `HOME=<profile>` +
    `AGENT_CLI_CREDENTIAL_STORE=file`, which writes `<profile>/.cursor/auth.json`
    (`accessToken` + `refreshToken`). Launchers must NOT move HOME (the agent runs the user's own
    shell commands): they export `CURSOR_AUTH_TOKEN` from that file with
    `AGENT_CLI_CREDENTIAL_STORE=memory` so the run can't touch the login keychain.
  - Neither supports API/proxy accounts, the API gateway pool, token-usage scanning, or
    auto-switch (no 5-hour window to react to).
- Additional Claude/Codex accounts are profile dirs under app data and are selected by exporting:
  - `CLAUDE_CONFIG_DIR=<profile>`
  - `CODEX_HOME=<profile>`
- The app does not wrap the real `claude`/`codex` binaries. It installs an idempotent shell hook in `~/.zshrc` and `~/.bashrc` if present.
- Per-account launcher commands are separate files in `~/.local/bin`, e.g. `claude-work`, `codex-pro`,
  `cursor-agent-work`, `opencode-alt`.
- The bare `cursor-agent` / `opencode` commands follow the selected account via shell FUNCTIONS in
  the hook (not exported variables): `XDG_DATA_HOME` and the Cursor token must stay scoped to that
  one invocation.
- Antigravity does not use profile env vars. It copy-swaps OAuth/profile keys inside the default IDE `state.vscdb`.

## Shared Config Rule

- Credentials must stay per account:
  - Claude OAuth is in macOS Keychain keyed by the profile-dir hash.
  - Codex OAuth is `auth.json` inside the profile.
  - API/proxy accounts use `api_key`, `config.toml`, or `settings.json` per account.
- Normal OAuth Claude/Codex accounts share user config/memory by symlinking profile entries back to default config:
  - Claude: `.claude.json`, `settings*.json`, `plugins`, `rules`, `commands`, `agents`.
  - Codex: `config.toml`, `rules`, `skills`, Codex memory/goals files.
- Session/history sharing is separate and always links back to the default config:
  - Claude: `projects`, `history.jsonl`.
  - Codex: `sessions`, `archived_sessions`, `history.jsonl`, and the merged `session_index.jsonl`.
  - Normal OAuth Codex launchers and the shell hook set `CODEX_SQLITE_HOME` to the default Codex
    home, so CLI profiles and Codex Desktop use one thread catalog while each profile keeps its own
    `CODEX_HOME` and `auth.json`. Never symlink SQLite files directly; keep memories/goals databases
    profile-local to avoid lock contention.
- Codex OAuth `auth.json` imports copy only validated OAuth profile files into Switcher's profile dir
  with private file permissions; never link the source credential. Account email is display metadata
  extracted from JWT claims only, not token content sent to the UI.
- `codex_import.rs` accepts one raw OAuth source: pasted JSON or a file. Parsing is local and has
  no provider calls or account writes. Return email/field names/duplicate status only; never echo
  token values in errors or persist the dialog's pasted JSON outside the private import flow.
- Do not apply shared config symlinks to API/proxy accounts, because their gateway/model/key config is intentionally account-specific.

## Frontend Notes

- `desktop.rs`, `desktop_rpc.rs` and `desktop_recovery.rs` manage explicit local desktop handoffs.
  Desktop and CLI selection are separate statuses. Never infer desktop identity from auth.json;
  verify the launched profile and the running backend's account/read email/workspace.
- Default handoffs wait for active turns. Unknown/unmanaged desktop execution homes wait until
  closed. Do not force-quit apps, interrupt unrelated CLI sessions, or reconstruct ephemeral/cloud
  or native client-owned tool runtimes. Native brands share one desktop data folder, so run one
  brand at a time. Never copy or symlink auth.json for desktop handoff.
- Recovery checkpoints are private app-data files, written atomically with fsync and owner-only
  permissions. Record interruption/send intent before RPC mutations. Resume only a turn interrupted
  by that operation; inspect history/acknowledgement after uncertain sends and never resend blindly.
- Codex quota must come from the selected identity's live endpoint. Shared rollout limits are not
  a valid fallback, since another account can have written them.
- `session_migration.rs` repairs known local catalog/history schemas additively using SQLite
  snapshots and private backups. Never replace existing threads or projection cursors, symlink
  live databases, or import enrollments/auth/daemon tables. Partial histories and unknown schemas
  need attention. Preserve the previously restored morning chats; do not continue completed work.

- `src/App.tsx` is the main UI: tool tabs, account cards, modals, auto-switch settings.
- `src/UsageView.tsx` renders token/cost usage.
- `src/OverlayApp.tsx` + `src/overlay.css` render the floating quota overlay; `src/main.tsx` picks it by window label.
- The overlay may collapse to a draggable logo bubble. While minimized, resize events must not
  overwrite its expanded geometry, and cursor pass-through must be disabled so it can expand.
- `src/QuotaPanelApp.tsx` + `src/quota-panel.css` render the menu-bar dropdown with all visible
  accounts, email, quota, filters, details and full-window actions. Left click opens it; right click
  retains the native menu. Hide it on blur/Escape instead of destroying the preloaded webview.
- `src/theme.ts` applies the single Michael Le blue appearance to all windows.
- `src/tauri.ts` wraps invoke calls and contains mock data for browser/dev fallback.
- `src/types.ts` mirrors Rust DTOs.

## Build/Test

- Frontend build: `npm run build`.
- Rust tests/checks: run inside `src-tauri`, e.g. `cargo test` or `cargo check`.
- Full dev app: `npm run tauri dev`.

## Guardrails

- Keep edits scoped; avoid changing shell hook semantics unless account switching requires it.
- Do not delete user profile dirs or CLI config unless the user explicitly asks.
- When changing shared-profile behavior, preserve credential isolation and skip API/proxy accounts.
- Codex API/proxy profiles must unset `CODEX_SQLITE_HOME` so their gateway-specific state stays isolated.
- App-local auto-prime uses the manual HTTP prime implementation, the same overlap guard and a
  durable five-hour claim recorded before sending. Skip hidden, API, locked and unknown-quota
  accounts. A successful Codex quota read for Team/Business/Pro with weekly quota but no five-hour
  bucket may send a greeting once per five-hour cooldown; report Hello sent without claiming a new
  window. Never rotate tokens or spawn a CLI/wake daemon to run automatic greetings.
- Prefer idempotent repair/migration during startup (`ManagedState::heal_active_profiles`) so existing accounts self-heal.
