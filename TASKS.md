# Michael Le Profiles — requested task index

The app's owner requested these tasks in the development chat. Private prompts and credentials are
not reproduced here. This index tracks the requested features without publishing the chat transcript.

The maintained specification and acceptance checks are in [docs/requirements.md](docs/requirements.md).
Update both files with relevant changes. Distinguish source support from installed-app validation
and release delivery; document remaining limitations and evidence with the corresponding index.

| Index | Requested task | Status |
| --- | --- | --- |
| 1 | Share Codex CLI/Desktop session history with the main `.codex` catalog | Implemented; 0.15.1 fixes desktop sidebar visibility across atomic saves and credential handoffs; OAuth credentials remain per profile |
| 2 | Import Codex accounts from `auth.json` | 0.15.4 normalizes rich-text clipboard spacing outside strings; preserves credential values and source files |
| 3 | Show account email in profiles and Usage sessions | Implemented where source metadata is available |
| 4 | Consolidate duplicate profiles and prevent credential conflicts | Duplicate profiles hidden; shared-token copies detected; original data retained |
| 5 | Fetch and display account quota | 0.15.4 sends the selected workspace header and explains rejected credentials; parsing/import is separate from live authentication |
| 6 | Quick quota access from the sidebar and richer full app | Implemented; sidebar now opens the menu-bar table |
| 7 | Give quota-reset credits a distinct icon from refresh | Implemented |
| 8 | Michael Le (duyle) identity, blue logo/theme and duyle.me link | Implemented; original author thanked in documentation |
| 9 | Update version, build, install and show the macOS app | 0.15.4 frontend/universal macOS build passed; verified DMG/ZIP delivered and the installed bundle replaced with 0.15.4, preserving existing account IDs. Manager not relaunched; native GUI and live desktop handoff were not exercised. Delivery evidence below |
| 10 | Native Liquid Glass, responsive UI, CSS and usage bars | Implemented |
| 11 | GitHub authentication, push and merge into the user’s fork | Current local/fork histories and upstream source integrated and pushed to main; verified source integration bf04cca. No version tag, GitHub Release, Homebrew update or installation performed for this update |
| 12 | List prior prompts and explain macOS refresh/install steps | Answered in the development chat |
| 13 | Dropdown table below the menu-bar icon; expand each account; full-screen UI | Implemented |
| 14 | Keep the floating overlay, improve its details and minimize like a pet | Implemented; draggable logo bubble, one-click expansion and upstream CPU/RAM/busiest-app footer |
| 15 | Automate a small Hello for eligible Pro/Team/Business accounts | Implemented; app-local scheduler, existing tokens, durable cooldown |
| 16 | Export credentials by account, provider or all, raw Codex auth, email and usage | Native Codex auth.json object/array export and selectable bulk JSON import implemented in source; no wrapper/config/history. Legacy backups retained. Synthetic round-trip, scope, permissions and changed-file selection checks; installed app unchanged |
| 17 | Apply the selected account to Codex.app and ChatGPT.app; confirm desktop identity separately | 0.15.2 resolves the owned backend and JSON daemon PID metadata; rejects unrelated CLI controllers. Reachable local handoffs supported; private/unmanaged desktop backends need a safe manual close/apply |
| 18 | Refresh email/quota immediately and show CLI/Desktop status separately | Implemented; immediate selected-account read, credential watcher and all-window broadcasts; shared-log fallback removed |
| 19 | Save and recover work interrupted by a desktop account switch | Guarded checkpoint/restart/native continuation implemented for supported reachable local desktop backends. Native delivery intent, preflight and observation-only retry added. Private/unmanaged backends remain unsupported; live native UI/tool recovery was not exercised |
| 20 | Recover missing local catalog entries and paginated history; preserve the four restored morning chats | 0.15.3 adds metadata-only chat search and original-chat opening during active work/pending handoffs, independent of idle migration. Shared metadata/archive state wins over profile copies; profile-only histories need later safe repair. Prior 47 catalog / 28 history imports and sidebar recovery remain preserved. Validation/build evidence recorded below |
| 21 | Maintain all requirements, task status and reusable migration prompts in this repository | Implemented; living specification, audit/implementation prompts and an AGENTS.md rule require updates alongside relevant changes |
| 22 | Paste and parse Codex credential JSON, alongside JSON file import | 0.15.4 repairs NBSP/Unicode indentation in paste and file sources with exact string/token preservation; local parsing and duplicate checks remain separate from live quota validation. Delivery evidence below |
| 23 | Import provider/all credential-backup JSON additively | Implemented in source for Codex/Claude OAuth and Codex/Claude API-proxy profiles; provider-scoped metadata-only preview, duplicate skip, destination-local profiles and private credentials. Production frontend build, Rust check and universal macOS build passed; no real credential import/Keychain write performed |
| 24 | Simplify macOS install and upgrades with DMG and Homebrew | Universal 0.15.4 DMG/ZIP built, verified and delivered locally. Homebrew/latest GitHub Release remain at [0.15.2](https://github.com/leduy-it/ai-switcher/releases/tag/v0.15.2); Cask/release workflow retained. Homebrew install/upgrade not exercised |
| 25 | Desktop-first quota switching, credential reload and same-session continuation | Source implemented: scoped identity/healthy quota selection, startup/direct desktop queue, supported native updater continuation with preflight and observation-only retry. 142 Rust tests passed, 1 authorized-live check ignored; frontend build and 7 isolated credential/restart/continuation scenarios passed. Private/unmanaged stdio and native live UI recovery remain unverified/unsupported; installed app unchanged |
| 26 | Native Codex renewal for desktop and CLI; distinguish expired access from logout | Implemented and independently reviewed in source; synthetic native-auth subprocess, identity/freshness and notification-flood deadline tests pass. Final Rust suite: 147 passed, 1 authorized-live check ignored; frontend build passed. Real-provider renewal and installed-app delivery not exercised |

Automatic Hello requires the app to remain running and the computer to be awake. It spends a small
amount of quota and only attempts a new five-hour window when live provider state allows it. Provider
errors, unknown email, and unattributed historical usage are not treated as successfully resolved data.

Version 0.14.0 includes tasks 17–22. The current unmanaged desktop and its active work were preserved;
live handoff, interruption and recovery continuation have not been exercised during this release.
Desktop synchronization is enabled for the selected brand on the owner's installation, with no
pending handoff at installation. Existing accounts and Hello cooldown records were retained; private
app/settings and session-migration backups remain outside the repository. No test suite was run.

Version 0.15.0 adds task 23 provider-backup restore and task 24 Homebrew/DMG installation support.
At the time of the 0.15.0 change, its universal DMG was a local build artifact; that change did not
publish a GitHub Release or update the Homebrew tap. The app was not installed over the currently installed 0.14.0 app. No real
credential JSON was imported, no macOS Keychain credential was written, and no test suite was run.

Version 0.15.1 fixes the additional desktop state needed to keep local chats visible when changing
credentials (tasks 1, 17 and 20). Frontend production build and three focused regression tests pass:
atomic desktop-save round trips, projectless chat recovery, legacy sharing backups, credential
isolation, repeatable merges and local-only catalog refresh. The release workflow runs these checks.
The patch is delivered through the tagged GitHub Release and universal macOS DMG (tasks 9, 11 and
24). Live credential switching and the installed desktop UI were not exercised; active desktop work
was left running. The installed Profiles application is unchanged until the release is installed.

Follow-up credential refresh investigation: the running installation was still 0.14.0 with
apply-on-selection disabled, so CLI selection did not request a desktop refresh. Source fixes for
tasks 17–19 add desktop-owned backend discovery, JSON daemon PID support and rejection of a
separate CLI daemon as the desktop controller. Seven focused Rust checks pass. A real Codex
subprocess scenario with synthetic OAuth and a loopback provider passes seven restart/continuation
checks, including same-thread recovery, second-credential inference and persisted retry
acknowledgement. The main desktop processes stayed running. Native desktop/private-transport tool
recovery remains unsupported. Version 0.15.2 is now installed and relaunched with the built binary
verified against the installed binary, all account IDs retained and desktop synchronization settings
preserved. A private app/settings rollback backup and verified universal DMG are retained locally.
Publication status is tracked in tasks 11 and 24.

## 0.15.3 local chat discovery delivery

- Source: metadata-only title/topic/folder/ID search over shared and known OAuth catalogs, including hidden profiles; original shared chat opens through the existing native desktop link. Search remains available during active work and pending/private-backend handoffs.
- Validation: three focused Rust regressions passed, including a live WAL writer, literal searches, topic/name metadata, shared archive precedence, unsupported catalogs and result limits. Frontend production build passed. Universal native .app built for arm64 and x86_64, bundle version confirmed as 0.15.3. ZIP integrity verified. DMG packaging failed because local macOS disk-image services are unavailable (hdiutil reported Device not configured; diskutil could not use DiskManagement). Native chat opening/switching was not exercised as a test with active user work.
- Installed application at the time of 0.15.3 delivery: 0.15.2 remained installed; superseded by the 0.15.4 installation below.
- Publication: source is tracked on [fix/find-local-chats](https://github.com/leduy-it/ai-switcher/tree/fix/find-local-chats); universal .app ZIP built locally. GitHub Release/Homebrew remain at the prior published version.

## 0.15.4 Codex clipboard import delivery

- Source: Unicode whitespace repair outside JSON strings for both paste/file sources; preserved token values and source files, selected-workspace quota header and actionable authentication failures. No automatic token renewal or credential replacement.
- Validation: six synthetic parser regressions and three workspace/authentication quota regressions passed; frontend production build passed. An opt-in live smoke check passed through the clipboard parser, private profile writer and production quota reader with explicitly authorized usable credentials; source and imported credential values remained unchanged with no token rotation. The universal native build passed for arm64 and x86_64. DMG checksums passed; read-only mounting confirmed bundle version 0.15.4 and a binary identical to the build. ZIP integrity/version and SHA-256 checks passed. The bundle is unsigned and not notarized; native GUI behavior was not inspected. Real rejected credentials cannot be made valid by formatting repair; cross-machine live success requires current usable credentials.
- Installed application: replaced the stopped manager at `/Applications/Michael Le Profiles.app` with the verified 0.15.4 bundle; installed/build binary hashes match and existing account IDs were retained. A private rollback bundle is outside the repository. The manager was not relaunched, so pending desktop operations and active Codex/ChatGPT work were not exercised.
- Publication: fix commit [5bad86a](https://github.com/leduy-it/ai-switcher/commit/5bad86a339a7edc49b542e9bd34488322844c697) pushed to `fix/codex-auth-import`. Verified `Michael-Le-Profiles-0.15.4-universal.dmg`, `.zip` and SHA-256 files delivered locally. No main merge, version tag, GitHub Release or Homebrew update was requested/performed for this patch.

## 2026-10-29 source integration delivery

- Source integration `bf04cca` pushed to fork main; remote ref matched local HEAD. All then-current local and remote branch heads are ancestors of main. Old feature branch trees matched their already-merged squash commits; their histories were recorded without reverting newer implementations. The upstream v0.1 prototype is superseded by the current source. Upstream CPU/RAM overlay and MIT license were merged normally.
- Final source validation: 147 Rust tests passed, 1 authorized-live credential test intentionally ignored; production frontend build passed. Seven isolated synthetic identity/interruption/restart/same-thread continuation scenarios passed. Independent reviews covered export/import, desktop recovery and native auth renewal; actionable findings were fixed with regressions.
- Current app/package source remains unreleased 0.15.4. No installed application was replaced, no active desktop was quit and no real credential was renewed. Native private/unmanaged stdio control, live native client-tool continuation and real-provider refresh grants remain explicit limitations/unverified cases.
