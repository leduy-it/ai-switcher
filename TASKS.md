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
| 3 | Show account email in profiles and Usage sessions | Implemented where source metadata is available; 0.16.2 retains known creator labels when a profile imports another login |
| 4 | Consolidate duplicate profiles and prevent credential conflicts | Duplicate profiles hidden; shared-token copies detected; original data retained |
| 5 | Fetch and display account quota | 0.15.4 sends the selected workspace header and explains rejected credentials; parsing/import is separate from live authentication |
| 6 | Quick quota access from the sidebar and richer full app | Implemented; sidebar now opens the menu-bar table |
| 7 | Give quota-reset credits a distinct icon from refresh | Implemented |
| 8 | Michael Le (duyle) identity, blue logo/theme and duyle.me link | Implemented; original author thanked in documentation |
| 9 | Update version, build, install and show the macOS app | Local 0.16.2 arm64 release built, signed/verified, installed and running; 12 account IDs and 19 credential/config file hashes preserved. Native layout/auth-preview checked. Public release/Homebrew unchanged |
| 10 | Native Liquid Glass, responsive UI, CSS and usage bars | Implemented |
| 11 | GitHub authentication, push and merge into the user’s fork | Existing local/fork/upstream histories integrated into main; 0.16.1 recovery and 0.16.2 same-profile auth import delivered through main. No version tag, GitHub Release or Homebrew update |
| 12 | List prior prompts and explain macOS refresh/install steps | Answered in the development chat |
| 13 | Dropdown table below the menu-bar icon; expand each account; full-screen UI | Implemented |
| 14 | Keep the floating overlay, improve its details and minimize like a pet | Implemented; draggable logo bubble, one-click expansion and upstream CPU/RAM/busiest-app footer |
| 15 | Automate a small Hello for eligible Pro/Team/Business accounts | Implemented; app-local scheduler, existing tokens, durable cooldown |
| 16 | Export credentials by account, provider or all, raw Codex auth, email and usage | 0.16.0 installed: native Codex object/array Copy JSON and Download JSON; selectable append-only import. Shared native generator, scope, private writes and changed-file checks validated. Legacy backups retained |
| 17 | Apply the selected account to Codex.app and ChatGPT.app; confirm desktop identity separately | 0.16.1 source: desktop-only native transport adapter, verified process chain/socket peer/cached auth/workspace. Existing private desktops require one safe close; ordinary CLI launchers remain independent |
| 18 | Refresh email/quota immediately and show CLI/Desktop status separately | Implemented; immediate selected-account read, credential watcher and all-window broadcasts; shared-log fallback removed |
| 19 | Save and recover work interrupted by a desktop account switch | Native GUI synthetic fixture confirms one original-chat continuation on the same controlled core. Durable checkpoints, target/last-turn revalidation and observation-only retry retained. Every native tool, cloud work and terminal/RAM state remain unverified |
| 20 | Recover missing local catalog entries and paginated history; preserve the four restored morning chats | 0.15.3 adds metadata-only chat search and original-chat opening during active work/pending handoffs, independent of idle migration. Shared metadata/archive state wins over profile copies; profile-only histories need later safe repair. Prior 47 catalog / 28 history imports and sidebar recovery remain preserved. Validation/build evidence recorded below |
| 21 | Maintain all requirements, task status and reusable migration prompts in this repository | Implemented; living specification, audit/implementation prompts and an AGENTS.md rule require updates alongside relevant changes |
| 22 | Paste and parse Codex credential JSON, alongside JSON file import | 0.15.4 repairs NBSP/Unicode indentation in paste and file sources with exact string/token preservation; local parsing and duplicate checks remain separate from live quota validation. Delivery evidence below |
| 23 | Import provider/all credential-backup JSON additively | Implemented in source for Codex/Claude OAuth and Codex/Claude API-proxy profiles; provider-scoped metadata-only preview, duplicate skip, destination-local profiles and private credentials. Production frontend build, Rust check and universal macOS build passed; no real credential import/Keychain write performed |
| 24 | Simplify macOS install and upgrades with DMG and Homebrew | Universal 0.15.4 DMG/ZIP built, verified and delivered locally. Homebrew/latest GitHub Release remain at [0.15.2](https://github.com/leduy-it/ai-switcher/releases/tag/v0.15.2); Cask/release workflow retained. Homebrew install/upgrade not exercised |
| 25 | Desktop-first quota switching, credential reload and same-session continuation | 0.16.1 source: verified-desktop quota polling, recent healthy replacement selection, saved-target checks and native managed transport. Actual native GUI synthetic fixture passed same-chat/model/cwd and one completed continuation; final build/install evidence below |
| 26 | Native Codex renewal for desktop and CLI; distinguish expired access from logout | Native auth-only isolation, proactive ten-minute renewal, private candidate retention and prioritized provider rejection messages implemented. 155 Rust tests passed, 1 live check ignored; frontend build passed. Live private before/after evidence confirmed two successful token rotations with readable quota; other profiles received native provider rejection. Results are account-specific |
| 27 | Copy JSON or Download JSON, with append-only bulk import | Shared Rust native auth object/array generator; clipboard secrets never enter webview state. Import serializes single/list writes and preserves existing credentials, pool and selection. Scope/append/failure regression tests passed |
| 28 | Import different native Codex auth into the same profile | 0.16.2 installed and relaunched; per-card file/paste preview, auth-only replacement/backup/rollback, stable profile, owner/duplicate/recovery guards and revision-bound quota effects. Synthetic regression tests and installed preview UI checked; no real auth replaced |

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

## 2026-10-29 local 0.16.0 installation and renewal follow-up

- Fixed idle auth startup blocking on shared history: native renewal now uses a private auth-only home. It retains changed/uncertain credentials on every post-native failure, rechecks the destination immediately before commit and preserves hard provider failures across cooldown. This is not atomic CAS against external clients.
- Single and bulk import serialize mutations, reject token-only duplicates and append new profiles without replacing existing credentials/pool/selection. Copy JSON and Download JSON share native object/array scope rules; clipboard writes occur in Rust, returning counts only.
- Validation: 155 Rust tests passed, 1 explicit live-auth test ignored; production frontend and arm64 app release builds passed. Independent review found no remaining blocking issue after credential conservation and duplicate checks were addressed.
- Installed and running local 0.16.0 with verified ad-hoc bundle signature and matching built/installed binary hash. Existing account IDs and credential bytes were preserved during replacement; private backup/manifest remain outside the repository. Native UI exposes both export actions and actual quota/renewal failures. The running ChatGPT desktop was left intact.
- Live diagnostics and regular startup reads confirmed real provider refresh-token reuse/rejection, alongside readable live quota on working credentials. Subsequent private before/after comparison confirmed two real managed profiles changed both access and refresh tokens, advanced token issuance/expiry and last_refresh, and returned live quota. Do not label other rejected refresh credentials recoverable by repeated submission. The installed private/unmanaged desktop still waits for a safe close before a verified handoff; native live tool/session continuation remains unverified.
- Source delivery is main; no public version tag, GitHub Release, Homebrew publication or universal bundle was produced for 0.16.0.

### Official auth research and live evidence correction

The initial conservative statement that real renewal was unverified was superseded by comparing the private pre-upgrade credential backup with current profile files: two profiles received new access/refresh tokens during native renewal and now have later JWT issuance/expiry plus last_refresh and readable live quota. No credential values or account identities are published here.

The remaining generic-error profile was independently probed through the installed native auth-only app-server: initialize succeeded, account/read returned account:null and native auth reported that it could not refresh and required sign-in. Its source and isolated auth remained unchanged. Other profiles explicitly reported reused or invalid refresh credentials. No newer retained renewal candidate was found in managed profiles. These are per-profile outcomes, not evidence that every 401 needs a fresh login.

Official references: [managed caching and credential storage](https://learn.chatgpt.com/docs/auth#login-caching), [account/read refreshToken semantics](https://learn.chatgpt.com/docs/app-server#1-check-auth-state), and [persist refreshed auth; avoid concurrent copies](https://learn.chatgpt.com/docs/auth/ci-cd-auth#operational-rules-that-matter). Native Codex owns the refresh grant; preserving its updated file is required. A stale original seed must not overwrite a renewed profile.

### 0.16.1 desktop recovery follow-up

Reread the owner requirements 1–27. Native JSON sharing remains native object/array with Copy/Download, single import and additive selectable bulk import (16, 22, 23, 27); credential renewal/isolation remains account-scoped (4, 5, 26). No account pool, credential, restored historical chat, global CLI launcher or shared catalog is replaced by this update.

The initial local-daemon environment flag was rejected by an actual native GUI test: native tool overrides force stdio, producing a different core from the observed daemon. The replacement desktop-only adapter preserves native configuration and tool transport on one owned core. Startup-policy commands are delegated; normal GUI SIGTERM cleans up the adapter child. Private record/socket permissions and live process/peer identity bind control to the GUI. Only an exclusive idle legacy destination daemon may stop; unrelated CLI/proxy clients are preserved.

Independent review found and corrected stale destination checks, stale native resume candidates, lost hard renewal error categories, repeated selection writes, daemon-only catalog verification and signal/CLI-proxy lifecycle issues. Compact UI fixes address task 10; status confirmation remains PID-bound and settings/actions reflect safe waiting versus interruption.

Validation and delivery are recorded separately below. Existing private desktop sessions are deliberately left running until one safe close; automatic handoff becomes controllable after a managed launch. The native synthetic fixture proves model-only continuation, not every native external tool or terminal/RAM/cloud restoration. Recent explicit quota-failure handling has focused regression coverage; real-account exhaustion is not manufactured for testing.

0.16.1 final source/build evidence: full `cargo test --locked -- --test-threads=1` passed 182 tests, with 3 opt-in checks ignored by default; the isolated native-daemon ownership check and native GUI ownership check passed separately. Frontend production build and 7 UI-state scenarios passed. Eight isolated native credential/restart scenarios passed. The actual native GUI fixture also passed using the signed release-bundle executable: one completed continuation with credential B on the same controlled chat/model/cwd, cached auth/workspace and Unix peer verification, followed by record/child cleanup. The local arm64 0.16.1 app bundle is built and ad-hoc signature verification passed. This does not validate every native tool or actual provider quota exhaustion.

Delivery limitation: the Mac is locked and computer-use could not unlock it. Native installed UI wide/narrow inspection and normal manager quit/install/relaunch are pending the owner's unlock. The installed/running Profiles app remains 0.16.0; the user's original private desktop was left running. Do not report the new build as installed or native visual QA as completed. Existing remote branch histories were already merged into main; the follow-up source is delivered through an ordinary main commit/push, without force-pushing or rewriting history.

### 0.16.2 same-profile auth import

The owner explicitly requested keeping a profile while importing another Codex auth. Added a separate per-card Import auth file/paste flow. Native bulk/Add imports keep their append-only behavior. Replacement preserves account identity within this app (ID/name/command/selection/config/history), updates the login metadata and invalidates old quota/desktop proof. Weekly reserve enabled/threshold persist; old derived lock state is cleared and the launcher is restored if possible, with a separate repair warning after successful credential import if necessary.

Preview hashes bind source bytes, target ID and current auth. Other-profile login/token duplicates, active owners and unresolved recovery are rejected. Replacement shares the profile renewal mutex and switch lock, retains private auth backups and rolls back unchanged writes on metadata failure. Credential revision guards protect both quota assignment and deferred weekly-lock/notification effects from old HTTP responses. Tests use private synthetic profiles only; no real account auth is replaced in development.

Same-profile replacement also retains old/new creator user-ID/email metadata without tokens, so existing Usage creator attribution survives a login change. Current native profile identities override saved labels; unknown creators stay unknown.

0.16.2 verification/delivery: full Rust suite passed 191 tests (3 opt-in checks ignored); frontend production build and 7 desktop UI-state scenarios passed. Independent review's deferred-lock and derived-lock findings were corrected and regression-tested. The selected-profile test verifies active-file bytes and history/config content remain unchanged and old/new creator labels remain known. The final arm64 release app was built, ad-hoc signed, installed over 0.16.0 and relaunched; native AX shows v0.16.2. All 12 account IDs and all 19 credential/config file hashes match the private pre-install manifest after startup and fake UI preview. The installed executable matches the verified bundle.

Native UI inspection confirmed the compact Codex summary, wrapped credential actions and file/paste replacement modal. Pasted synthetic preview@example.invalid JSON displayed the incoming email and enabled the explicit import action; Cancel discarded it. No real credential was replaced in this check. Exact minimum-width resizing was not confirmed by automation. The original private desktop remains running and its waiting handoff remains safe; after work finishes it still needs one safe close to adopt the managed transport. This supersedes the earlier locked-Mac install limitation: Profiles 0.16.2 is now installed/running. GitHub Release/Homebrew publication is unchanged; source is delivered to remote main.
