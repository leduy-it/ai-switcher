# Michael Le Profiles — requested task index

The app's owner requested these tasks in the development chat. Private prompts and credentials are
not reproduced here. This index tracks the requested features without publishing the chat transcript.

The maintained specification and acceptance checks are in [docs/requirements.md](docs/requirements.md).
Update both files with relevant changes. Distinguish source support from installed-app validation
and release delivery; document remaining limitations and evidence with the corresponding index.

| Index | Requested task | Status |
| --- | --- | --- |
| 1 | Share Codex CLI/Desktop session history with the main `.codex` catalog | Implemented; OAuth credentials remain per profile |
| 2 | Import Codex accounts from `auth.json` | Implemented |
| 3 | Show account email in profiles and Usage sessions | Implemented where source metadata is available |
| 4 | Consolidate duplicate profiles and prevent credential conflicts | Duplicate profiles hidden; shared-token copies detected; original data retained |
| 5 | Fetch and display account quota | Implemented; provider auth/read failures remain visible |
| 6 | Quick quota access from the sidebar and richer full app | Implemented; sidebar now opens the menu-bar table |
| 7 | Give quota-reset credits a distinct icon from refresh | Implemented |
| 8 | Michael Le (duyle) identity, blue logo/theme and duyle.me link | Implemented; original author thanked in documentation |
| 9 | Update version, build and install the macOS app | Built and installed 0.14.0; installed binary matches the build and the new process is running. Frontend production build, Rust check and native macOS build passed. Native UI inspection is still unverified because the Mac was locked |
| 10 | Native Liquid Glass, responsive UI, CSS and usage bars | Implemented |
| 11 | GitHub authentication, push and merge into the user's fork | Fork authentication configured; 0.14.0 published in [release PR #4](https://github.com/leduy-it/ai-switcher/pull/4), which records merge status |
| 12 | List prior prompts and explain macOS refresh/install steps | Answered in the development chat |
| 13 | Dropdown table below the menu-bar icon; expand each account; full-screen UI | Implemented |
| 14 | Keep the floating overlay, improve its details and minimize like a pet | Implemented; draggable logo bubble and one-click expansion |
| 15 | Automate a small Hello for eligible Pro/Team/Business accounts | Implemented; app-local scheduler, existing tokens, durable cooldown |
| 16 | Export credentials by provider or all, raw Codex auth, email and usage | Implemented; private JSON export with unavailable-source notes |
| 17 | Apply the selected account to Codex.app and ChatGPT.app; confirm desktop identity separately | Implemented; queued controlled handoff and local backend identity verification. Existing unmanaged desktops wait until closed |
| 18 | Refresh email/quota immediately and show CLI/Desktop status separately | Implemented; immediate selected-account read, credential watcher and all-window broadcasts; shared-log fallback removed |
| 19 | Save and recover work interrupted by a desktop account switch | Implemented for reachable local desktop sessions; private durable checkpoint, safe wait, guarded same-thread continuation and uncertain-delivery checks. Unsupported/cloud/ephemeral/native-tool runtimes need attention |
| 20 | Recover missing local catalog entries and paginated history; preserve the four restored morning chats | Four morning chats preserved; additive repair imported 47 additional catalog records and 28 histories from two idle profiles. Repeat repair inserted nothing and found no history conflicts; one existing cursor has pending settings metadata. GUI repair implemented; private backups retained |
| 21 | Maintain all requirements, task status and reusable migration prompts in this repository | Implemented; living specification, audit/implementation prompts and an AGENTS.md rule require updates alongside relevant changes |
| 22 | Paste and parse Codex credential JSON, alongside JSON file import | Implemented and included in installed 0.14.0; email/field-name preview, local format validation and duplicate checks before import. Production/native builds and Rust check passed; no real credential import or native GUI inspection performed |
| 23 | Import provider/all credential-backup JSON additively | Implemented in source for Codex/Claude OAuth and Codex/Claude API-proxy profiles; provider-scoped metadata-only preview, duplicate skip, destination-local profiles and private credentials. Production frontend build, Rust check and universal macOS build passed; no real credential import/Keychain write performed |
| 24 | Simplify macOS install and upgrades with DMG and Homebrew | Universal 0.15.0 DMG built at `src-tauri/target/universal-apple-darwin/release/bundle/dmg/Michael Le Profiles_0.15.0_universal.dmg`; x86_64+arm64 and DMG checksum verified. Cask/workflow added. Homebrew audit couldn't run because this Mac's Command Line Tools are outdated; tap/release not published |

Automatic Hello requires the app to remain running and the computer to be awake. It spends a small
amount of quota and only attempts a new five-hour window when live provider state allows it. Provider
errors, unknown email, and unattributed historical usage are not treated as successfully resolved data.

Version 0.14.0 includes tasks 17–22. The current unmanaged desktop and its active work were preserved;
live handoff, interruption and recovery continuation have not been exercised during this release.
Desktop synchronization is enabled for the selected brand on the owner's installation, with no
pending handoff at installation. Existing accounts and Hello cooldown records were retained; private
app/settings and session-migration backups remain outside the repository. No test suite was run.

Version 0.15.0 adds task 23 provider-backup restore and task 24 Homebrew/DMG installation support.
The 0.15.0 universal DMG is a local build artifact; this change does not publish a GitHub Release or
update the Homebrew tap. The app was not installed over the currently installed 0.14.0 app. No real
credential JSON was imported, no macOS Keychain credential was written, and no test suite was run.
