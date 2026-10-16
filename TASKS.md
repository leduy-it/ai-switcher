# Michael Le Profiles — requested task index

The app's owner requested these tasks in the development chat. Private prompts and credentials are
not reproduced here. This index tracks the requested features without publishing the chat transcript.

The maintained specification and acceptance checks are in [docs/requirements.md](docs/requirements.md).
Update both files with relevant changes. Distinguish source support from installed-app validation
and release delivery; document remaining limitations and evidence with the corresponding index.

| Index | Requested task | Status |
| --- | --- | --- |
| 1 | Share Codex CLI/Desktop session history with the main `.codex` catalog | Implemented; 0.15.1 fixes desktop sidebar visibility across atomic saves and credential handoffs; OAuth credentials remain per profile |
| 2 | Import Codex accounts from `auth.json` | Implemented |
| 3 | Show account email in profiles and Usage sessions | Implemented where source metadata is available |
| 4 | Consolidate duplicate profiles and prevent credential conflicts | Duplicate profiles hidden; shared-token copies detected; original data retained |
| 5 | Fetch and display account quota | Implemented; provider auth/read failures remain visible |
| 6 | Quick quota access from the sidebar and richer full app | Implemented; sidebar now opens the menu-bar table |
| 7 | Give quota-reset credits a distinct icon from refresh | Implemented |
| 8 | Michael Le (duyle) identity, blue logo/theme and duyle.me link | Implemented; original author thanked in documentation |
| 9 | Update version, build, install and show the macOS app | 0.15.2 frontend and universal macOS build passed; seven focused Rust regressions and seven isolated subprocess scenarios passed. Installed and relaunched 0.15.2 with accounts/settings preserved; main desktop stayed running. Live desktop handoff was not exercised |
| 10 | Native Liquid Glass, responsive UI, CSS and usage bars | Implemented |
| 11 | GitHub authentication, push and merge into the user's fork | Fork authentication configured; 0.15.2 patch tracked in [PR #6](https://github.com/leduy-it/ai-switcher/pull/6) for main and the [tagged release](https://github.com/leduy-it/ai-switcher/releases/tag/v0.15.2). Earlier 0.14.0 work remains recorded in PR #4 |
| 12 | List prior prompts and explain macOS refresh/install steps | Answered in the development chat |
| 13 | Dropdown table below the menu-bar icon; expand each account; full-screen UI | Implemented |
| 14 | Keep the floating overlay, improve its details and minimize like a pet | Implemented; draggable logo bubble and one-click expansion |
| 15 | Automate a small Hello for eligible Pro/Team/Business accounts | Implemented; app-local scheduler, existing tokens, durable cooldown |
| 16 | Export credentials by provider or all, raw Codex auth, email and usage | Implemented; private JSON export with unavailable-source notes |
| 17 | Apply the selected account to Codex.app and ChatGPT.app; confirm desktop identity separately | 0.15.2 resolves the owned backend and JSON daemon PID metadata; rejects unrelated CLI controllers. Reachable local handoffs supported; private/unmanaged desktop backends need a safe manual close/apply |
| 18 | Refresh email/quota immediately and show CLI/Desktop status separately | Implemented; immediate selected-account read, credential watcher and all-window broadcasts; shared-log fallback removed |
| 19 | Save and recover work interrupted by a desktop account switch | Implemented for reachable local desktop sessions; private durable checkpoint, safe wait, guarded same-thread continuation and uncertain-delivery checks. Unsupported/cloud/ephemeral/native-tool runtimes need attention |
| 20 | Recover missing local catalog entries and paginated history; preserve the four restored morning chats | 0.15.3 adds metadata-only chat search and original-chat opening during active work/pending handoffs, independent of idle migration. Shared metadata/archive state wins over profile copies; profile-only histories need later safe repair. Prior 47 catalog / 28 history imports and sidebar recovery remain preserved. Validation/build evidence recorded below |
| 21 | Maintain all requirements, task status and reusable migration prompts in this repository | Implemented; living specification, audit/implementation prompts and an AGENTS.md rule require updates alongside relevant changes |
| 22 | Paste and parse Codex credential JSON, alongside JSON file import | Implemented since 0.14.0 and included in installed 0.15.2; email/field-name preview, local format validation and duplicate checks before import. Production/native builds and Rust check passed; no real credential import or native GUI inspection performed |
| 23 | Import provider/all credential-backup JSON additively | Implemented in source for Codex/Claude OAuth and Codex/Claude API-proxy profiles; provider-scoped metadata-only preview, duplicate skip, destination-local profiles and private credentials. Production frontend build, Rust check and universal macOS build passed; no real credential import/Keychain write performed |
| 24 | Simplify macOS install and upgrades with DMG and Homebrew | Universal 0.15.2 DMG built and verified; installed and relaunched locally. Versioned/stable assets distributed through the [0.15.2 release](https://github.com/leduy-it/ai-switcher/releases/tag/v0.15.2); Cask and release workflow retained. Homebrew installation/audit was not exercised |

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
- Installed application: 0.15.2 remains installed.
- Publication: source is tracked on [fix/find-local-chats](https://github.com/leduy-it/ai-switcher/tree/fix/find-local-chats); universal .app ZIP built locally. GitHub Release/Homebrew remain at the prior published version.
