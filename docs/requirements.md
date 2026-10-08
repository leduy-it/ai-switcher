# Michael Le Profiles requirements

Michael Le (duyle) needs these features preserved in Michael Le Profiles and in any app that replaces it. The priority is shared local Codex history, separate credentials, visible account email and accurate quota, followed by desktop switching, recovery, automation and the macOS interface.

This is the repository's living requirements reference. Preserve the established requirement indices 1–22. Requirements describe the intended behavior; [TASKS.md](../TASKS.md) records implementation progress, validation limits and release delivery. Record new requirements with new indices instead of renumbering existing ones.

## Keeping the requirements current

For every relevant feature, behavior, provider, interface, credential/session storage, migration or release change:

1. Read this reference and [TASKS.md](../TASKS.md) before implementation.
2. Update the affected requirements and acceptance checks in the same change as the implementation. Record the latest user decision and identify a superseded decision explicitly.
3. Update the matching task status and material limitations. Distinguish code implemented, validation performed, installed version and published release; do not infer one from another.
4. Append a new index for a new requirement. Keep removed or superseded indices with their disposition so migration audits cannot silently omit them.
5. Update the relevant README/changelog and reusable prompts when their statements or coverage change.
6. Review the documentation against the final behavior before reporting completion. If no requirement changed, preserve the specification and update only the affected progress/evidence.

This rule is also in [AGENTS.md](../AGENTS.md). Repository files are authoritative. Personal notes, exported copies and Pages are snapshots unless explicitly refreshed from the repository. Keep tokens, real credential exports, private session contents and machine-specific recovery manifests out of these files.

## Scope and latest decisions

- Support Codex CLI and the local coding backend of both installed desktop brands, Codex.app and ChatGPT.app. Desktop identity and CLI selection must be shown separately.
- Use the main Codex session store for local OAuth history, while credentials remain in separate account profiles.
- The latest appearance is one Michael Le Profiles identity: Duy Lê / Michael Le (duyle), blue logo and blue Liquid Glass, with a link to https://duyle.me.
- The earlier request to preserve an original theme alongside a Michael theme was superseded. Hoàng Phan remains thanked and referenced in documentation; a second maintained author/theme mode is no longer required.
- Keep the menu-bar dropdown and the floating overlay as separate interfaces. Both need account details; the full app is the larger management interface.
- Automatic Hello supplements the manual “Bắt đầu phiên 5 giờ” control.
- Migration must preserve existing accounts, history and supported provider features. Record unsupported features explicitly rather than claiming complete parity.
- A shared local catalog does not grant access to cloud conversations belonging to another account. Missing provider email, quota or history attribution remains unknown.

## Reading current support

Consult [TASKS.md](../TASKS.md) for the current status of every index, [README.md](../README.md) for the supported interfaces/providers, and [CHANGELOG.md](../CHANGELOG.md) for release changes.

Source implementation, installed-app validation and release publication are separate facts. A feature described in source or a successful build is not proof that the running application contains it. Unknown identity, quota, historical attribution or runtime capability must remain visible rather than being described as resolved.

## Requirement index

| Index | Required behavior or delivery |
| --- | --- |
| 1 | Share Codex CLI and Desktop local sessions through the main Codex store |
| 2 | Import Codex accounts from auth.json |
| 3 | Show which email owns each credential and Usage session |
| 4 | Consolidate duplicate logins and prevent credential conflicts |
| 5 | Fetch each account’s current quota and usage |
| 6 | Open quick quota access from the top sidebar, with more detail in the full app |
| 7 | Give reset credits a different icon from refresh |
| 8 | Michael Le identity, blue theme/logo, duyle.me, author credit in docs |
| 9 | Update version, build, install and show the new macOS application |
| 10 | Native Liquid Glass, better CSS, responsive layouts and usage bars |
| 11 | Publish to the user’s GitHub fork and merge requested releases |
| 12 | Preserve requested-task history and explain refreshing the installed app |
| 13 | Menu-bar dropdown table, row expansion and full-app/full-screen action |
| 14 | Floating overlay with detailed rows and easy pet-like minimization |
| 15 | Automatic small Hello for eligible accounts, including Pro and Team/Business |
| 16 | Export one provider or all credentials with raw auth, email and usage |
| 17 | Apply the selected account to either installed desktop brand and confirm its identity |
| 18 | Refresh email/quota immediately and keep every window consistent |
| 19 | Checkpoint and recover local work interrupted by a desktop account switch |
| 20 | Restore older sessions and their complete paginated history |
| 21 | Maintain repository requirements, task status and reusable agent prompts |
| 22 | Paste and parse Codex credential JSON, alongside JSON file import |

## 1 Shared Codex local history

Requirements:

- Discover the actual default Codex home, any CODEX_HOME override, shell aliases/symlinks, Switcher-managed profiles, configured sqlite_home and the running desktop/backend’s storage paths.
- Normal OAuth profiles share session rollouts, archived sessions, prompt history and the merged session index with the main store.
- CLI, desktop and backend use the same thread catalog. In the current implementation CODEX_HOME selects credentials and CODEX_SQLITE_HOME selects the common catalog. A config-level override must be accounted for.
- Preserve project/sidebar state as well as thread titles, thread IDs, timestamps, creator identity, archived/pinned state and paginated turn/item history.
- Keep profile credentials separate. Keep API/proxy configuration separate where it has provider-specific keys, models or gateways.
- Merge old profile catalogs before redirecting them; linking the session directory alone does not restore every catalog/history entry.
- Do not symlink live SQLite files. Use supported storage overrides and consistent, schema-aware database migration.

Acceptance: create or select known local threads using two OAuth profiles and inspect them in CLI and Desktop. Their IDs, older pages and archived history must remain reachable after switching. Include existing threads from before migration. Perform live switching only in an authorized validation window.

## 2 Codex auth import

Requirements:

- Add an account from an existing Codex OAuth auth.json, including file selection in the GUI.
- Validate the expected OAuth fields and reject unsupported or malformed sources with a useful error.
- Copy the validated source into the new account’s private profile; leave the source file untouched.
- Extract display email and provider identity when available. Evaluate duplicate login identity before adding another active credential copy.
- Keep token values out of previews, logs and error text. Raw pasted JSON may exist only in the open import dialog until it is cleared, closed or imported; file-import token values stay in the backend.

Acceptance: a supported import creates the correct profile/email and does not modify the source. An invalid import creates no usable account. An auth.json import is not a claim that all provider backup formats can already be imported.

## 3 Email and usage ownership

Requirements:

- Display available account email in account cards, selectors, quota tables, overlay rows and export metadata.
- Usage sessions show the account email that actually created/ran them when creator metadata can be matched.
- Use user identity plus workspace context as needed; different users in one Team workspace must not be treated as duplicates.
- Historical rows without reliable attribution show Unknown account or an equivalent explicit state. Do not label them with the account selected today.
- Show selected CLI identity separately from a running desktop identity confirmed by the backend.

Acceptance: known session creators retain their email after selecting a different account. Unknown creator/email stays visibly unknown.

## 4 Credential isolation and duplicate consolidation

Requirements:

- Maintain one canonical managed credential profile per provider login and relevant workspace identity. Redirect duplicate managed entries to the canonical profile or keep duplicates disabled with a clear explanation.
- Detect duplicate email/user identity and exact shared-token copies without publishing token values.
- Consolidation preserves original profile data in a backup and preserves their sessions.
- Keep auth.json and other credential sources account-specific. Share eligible configuration and history independently.
- Do not rotate/refresh credentials merely to read quota or send Hello. If refresh is needed for login, respect the provider’s rotation behavior and active clients.
- Track which desktop/backend actually uses which profile before changing it. Unmanaged clients using old copies must be identified as unresolved.
- Separate credentials cannot by themselves guarantee no conflicts if multiple independent copies of the same refresh token remain active.

Acceptance: inventory all managed and unmanaged copies; report the canonical owner, aliases/disabled copies and remaining risks without secrets. Two entries sharing the same login must not silently act as independent refresh-token owners.

## 5 Quota and usage

Requirements:

- Read quota for each eligible provider/account and show its reported plan, available windows, usage/remaining percentages and reset times.
- Read Codex live quota using the selected identity. Shared rollout logs from another account are not an acceptable live-quota fallback.
- Refresh after selection and credential changes, and support manual refresh.
- Show timestamps, errors, rate limits and stale cached values clearly; unknown is not zero usage.
- Preserve token/cost reports and provider-specific quota definitions. A missing five-hour bucket on Pro/Team/Business is not automatically a read failure.

Acceptance: the displayed account/email and quota source agree. Authentication/network failures remain visible and cannot masquerade as another account’s successful read.

## 6 Sidebar quick access

Requirements:

- The top sidebar quota shortcut opens the preloaded quick quota dropdown.
- Quick access shows useful account/email/plan/usage information immediately.
- An explicit action opens the full app for detailed management.

Acceptance: one click on the shortcut exposes the table, and full-app navigation opens the correct view.

## 7 Reset and refresh controls

Requirements:

- Give quota-reset credits a distinct reset symbol; refreshing quota uses a separate refresh symbol.
- Explain available reset-credit counts and provider-reported expiry information.
- Do not imply that Refresh or Hello purchases, redeems or refills quota.

Acceptance: the two actions are distinguishable by icon and label. Any credit redemption is explicit and confirmed under the destination app’s policy.

## 8 Michael Le identity

Requirements:

- Present the app as Michael Le Profiles, maintained by Michael Le (duyle) / Duy Lê.
- Use the blue logo and blue theme consistently in the main app, menu bar, dropdown, overlay, loading states, dialogs and About/powered-by surfaces.
- Reference https://duyle.me wherever the Michael profile attribution belongs.
- Thank Hoàng Phan and reference the original project in documentation.
- Preserve legacy app-data and credential/keychain identifiers if changing them would lose saved accounts; internal compatibility names need not appear as product branding.

Acceptance: visible branding is consistent and old saved accounts remain accessible after rebranding. The retired original-theme mode is not required.

## 9 Version build and installation

Requirements:

- Keep package, backend, bundle and displayed About versions aligned.
- Build the application, install the new bundle into the application users actually launch, and reopen that bundle.
- Inspect its reported version and representative changed surfaces: branding, theme, dropdown, overlay and account controls.
- Explain how to replace/reopen an old macOS app. A source change or browser/dev preview is insufficient installation evidence.

Acceptance: the running installed app reports the intended version and shows the changed interface. Record artifact path, installed bundle path and validation result separately.

## 10 Liquid Glass and responsive interface

Requirements:

- Use native macOS Liquid Glass where supported, with a suitable native material fallback.
- Apply consistent blue glass styling to quota and usage bars, account cards, Settings, Usage and other management views.
- Keep text readable, contrast usable and status/usage understandable without relying only on color.
- Adapt layouts to small and large windows; long emails and tables remain accessible without clipping.
- Respect accessibility/system motion preferences and keep drag, resize, scrolling and keyboard controls usable.

Acceptance: inspect wide/narrow window layouts and long-account rows in all three surfaces. Bars display the same values as their labels.

## 11 Repository and release delivery

Requirements:

- Keep changes in the user’s chosen fork and preserve original attribution.
- Record the target repository, release version, commit/PR and publication status.
- Preserve the existing authorization from the active implementation conversation; a copied checklist itself does not authorize pushing, merging or publishing a new migration.

Acceptance: a requested release can be tied to the installed artifact and its actual merged commit. For the original app, the known fork is leduy-it/ai-switcher.

## 12 Prompt history and app refresh guidance

Requirements:

- Keep a numbered record of requested features, completion state and material limits.
- Preserve changes of direction so an agent applies the latest requirement, especially the single Michael identity and both desktop brands.
- Keep raw private prompts/session transcripts out of public repository documentation unless specifically requested.
- Provide concise macOS refresh/install guidance when the installed app remains old.

Acceptance: each request maps to a stable requirement number, including superseded decisions and open work.

## 13 Menu bar dropdown table

Requirements:

- Clicking the menu-bar icon opens an anchored table directly below it; it must not merely launch the full app.
- Show visible accounts across providers with email, plan and relevant quota windows.
- Filter by provider and search/email. Expand individual rows for reset times, commands, errors and actions.
- Provide refresh/switch actions, reset-credit details, full-app access and full-screen access.
- Keep useful native quick actions on right click. Escape or blur hides the dropdown and preserves its preloaded state.
- Apply the same refreshed account snapshot and Michael glass style as the main app.

Acceptance: anchoring works on the active display, details expand without clipping, and switching/refresh updates the same account across surfaces.

## 14 Floating overlay

Requirements:

- Keep the optional always-on-top quota overlay alongside the dropdown.
- Support dragging/resizing, account selection, email and expandable detail.
- Minimize into a small draggable logo bubble, similar to a desktop pet; one click expands it.
- Preserve expanded geometry while minimized and clamp restored geometry to the usable screen.
- Keep the bubble clickable even if an expanded view supports click-through mode.
- Preserve available opacity/hover behavior, placement across Spaces and Settings controls.

Acceptance: minimize, drag and expand return to a reachable usable panel. The overlay can be disabled independently of quick access.

## 15 Automatic Hello and manual prime

Requirements:

- Keep the manual “Bắt đầu phiên 5 giờ” action with a clear explanation: one minimal request may begin a provider-reported window and uses a small amount of quota.
- Offer app-local automation with enable/pause and eligible-account selection, including Pro and Team/Business.
- Validate current live quota before sending. Skip hidden, API/proxy, locked, failed/unknown-quota and otherwise ineligible accounts.
- Serialize manual and automatic attempts. Persist the attempt before sending and apply a durable five-hour cooldown, including uncertain delivery.
- Use the account’s existing credential with a minimal HTTP Hello; do not spawn a CLI, rotate a token or install a wake daemon for this automation.
- For accounts reporting weekly quota without a five-hour bucket, use the cooldown and report Hello sent. Claim a new window only when the provider reports it.
- The current scheduler requires the app running and the computer awake. Hello does not refill quota or reset weekly limits.

Acceptance: enable/pause and account filters work; a restart or uncertain response cannot duplicate a greeting within the cooldown. Any validation that sends a Hello must be explicitly authorized because it consumes quota.

## 16 Credential export

Requirements:

- Export one provider or all providers from the GUI, including hidden accounts.
- Include a format/schema version, export timestamp, provider/profile identity, available account email/plan, relevant settings, available quota and usage snapshot with its timestamp.
- Include raw credential sources and parsed fields, including raw Codex auth.json when available. Preserve provider distinctions and field names needed by a future importer.
- Report unavailable credential sources/fields explicitly; do not claim completeness where keychain/provider access was unavailable.
- Save atomically with private file permissions; do not send raw tokens through frontend state, logs or public documentation.
- Treat exports as secret-bearing backups. A future migration should validate its importer and secure transfer before moving real credentials.

Acceptance: an authorized export matches the chosen scope and preserves raw sources plus metadata. The report distinguishes current live data, stale/unknown usage and missing sources. Export support alone does not prove full backup restore support.

## 17 Desktop account switching

Requirements:

- Offer Codex.app and ChatGPT.app as separate launch targets where their installed local coding backends are supported.
- Provide Apply desktop and an opt-in apply-on-selection setting.
- Detect actual runtime profile/catalog paths; compare desired identity with the running backend’s account identity, email and workspace.
- Wait for active work by default. Use normal macOS quit and a controlled relaunch, with explicit profile/catalog and desktop-data paths.
- Preserve session/sidebar/project state. Never copy or symlink auth.json into a shared credential location to perform a handoff.
- If two installed brands share the same live data directory, use one at a time and disclose that limitation.
- Unknown/unmanaged execution profiles wait for the user to finish and close them. Do not interrupt unrelated CLI clients or force-quit active apps.
- Unsupported API/proxy/cloud/client configurations show a clear need for attention.

Acceptance: after an authorized handoff, the actual backend identity matches the selected account and reports when it was confirmed. Changing an auth file alone is not confirmation.

## 18 Immediate and consistent refresh

Requirements:

- Refresh the selected account’s live email/quota immediately after selection.
- Watch relevant credential changes from CLI or Desktop while the manager runs, without logging token content.
- Refresh only the affected identity and broadcast the updated snapshot to main app, dropdown and overlay.
- Display CLI selected, desktop pending/waiting, desktop confirmed and failure states separately.
- Invalidate a stale desktop identity confirmation after a credential change or failed verification.
- With desktop sync enabled, an external CLI selection or selected-profile credential mismatch queues
  a controlled reload. Token changes that still match the backend identity do not restart it.
  Existing pending/error recovery operations remain available for explicit resolution.

Acceptance: an authorized account switch shows the new selected quota promptly. The UI cannot label a still-running old desktop identity as already switched.

## 19 Checkpoint and recover interrupted work

Requirements:

- Default to waiting for active turns to finish.
- An explicit Switch now and recover action may checkpoint and interrupt eligible local desktop work.
- Save a private durable checkpoint before interruption: operation, host/app, thread/turn, cwd/worktree, old/new profile, model, execution/approval/sandbox policies, terminal metadata and last recorded results.
- Confirm the interruption and new desktop identity before continuing in the original thread.
- Preserve execution context; ask the resumed agent to inspect actual files/processes and continue after the last completed step.
- Persist send intent and acknowledgement. Check original history after uncertain delivery and never blindly resend a continuation.
- Do not automatically resume completed work, a user-stopped/resumed session, pending approvals, inaccessible cloud work, ephemeral sessions or unsupported client-owned tool runtimes. Uncertain external effects need attention.
- Show waiting, checkpointing, switching, restoring, continuation acknowledged and error states per session. Provide cancellation while waiting, original-session access and safe retry.
- Continuation acknowledgement is not task completion; terminal/RAM state is not promised restored.

Acceptance: an authorized validation on a disposable eligible session retains thread ID/context, sends at most one continuation and does not interfere with another session. Unsupported cases fail visibly without duplicate actions.

## 20 Restore and migrate older session catalogs

Requirements:

- Inventory all old profile homes and discover missing local threads even when rollout files are already in the main session directory.
- Merge the catalog and paginated turn/item history needed by the installed client, not just session_index.jsonl.
- Preserve IDs, titles, creator identity, original timestamps, policy/config metadata and archive/pin/project relationships where supported.
- Use consistent SQLite snapshots/backups and schema/capability checks. Avoid copying live database files as if they were a complete backup.
- Make migration additive and repeatable. Do not overwrite valid current history, regenerate completed turns or send continuation prompts to restored completed threads.
- Record a private manifest of sources, destinations, inserted/skipped/conflicting entries, counts and backup/rollback locations.
- Verify old history pages and archive access after redirection.
- Provide a GUI repair action for known OAuth profile catalogs, including hidden originals. Check
  that source backends are idle, resolve their short socket symlinks and use SQLite-managed WAL
  connections and snapshots. Import only supported local catalog/history/organization tables.
- Distinguish missing/changed history rows from a cursor behind a trailing metadata event. Existing
  destination cursors remain intact; pending metadata is reported for the native backend to replay.

The application must expose a repair result with inserted/skipped/conflicting records and unsupported cases. Existing completed threads stay completed; recovering their visibility does not authorize continuing their work.

Acceptance: compare source/destination counts and inspect older pages of representative restored threads. A second migration changes nothing for already imported records and reports conflicts instead of overwriting them.

## 21 Living requirements and agent prompts

Requirements:

- Keep this complete numbered specification in the repository as docs/requirements.md and update it with every relevant implementation or requirement change.
- Track current progress and limitations in TASKS.md using the same stable indices.
- Supply a reusable audit prompt and a separate implementation prompt under docs/prompts.
- Every future audit covers every requirement and baseline ID in the current specification and lists Supported, Partial, Missing, Unsupported or Not verified with evidence.
- Keep source implementation, installed behavior and release delivery separate.
- Append new requirement indices; retain the disposition of superseded or retired ones.
- Update the prompts when scope changes and require future agents to follow the maintenance rule in AGENTS.md.

Acceptance: another agent can read the repository documents without the development chat, assess a replacement app and produce a complete gap list. A relevant feature change includes the matching requirement/task/documentation updates.

## 22 Paste and parse Codex credential JSON

Requirements:

- The Codex account dialog offers a Paste JSON box and a separate Import file option. Both accept raw Codex OAuth auth.json and use the same parser and isolated-profile import flow.
- Parsing checks JSON syntax, OAuth token fields and a 1 MB limit without contacting the provider or creating an account. Show the available email, token field names and duplicate status; never display token values in the preview or error text.
- Editing the pasted JSON or changing source invalidates its preview. Import requires a successfully parsed, nonduplicate source and revalidates it in the backend before creating the profile.
- Keep pasted credentials only in the open dialog until import. Never store them in browser storage, logs or public files. Imported credentials use the existing private per-account storage and permissions; selected source files remain untouched.
- API keys use the existing API / Proxy form. An exported multi-provider backup is not a raw auth.json and needs a separately specified restore flow.

Acceptance: both source choices are visible, parsing creates no account and makes no quota request, malformed/oversized/non-OAuth inputs receive a useful error without secret contents, email may remain unknown, duplicates cannot be imported, and a valid import uses isolated private credentials with shared eligible session storage.

## Existing app features to preserve when applicable

These are baseline features documented by the existing app, rather than additional requests newly made in this side conversation. A migration audit should list each by its baseline ID and report intentional exclusions.

| Baseline ID | Existing support |
| --- | --- |
| B01 | Provider account management: add, sign in, rename, hide/unhide, remove and select accounts; retain the machine-default account |
| B02 | Claude Code and Codex account launchers, bare-command selection, idempotent zsh/bash integration and aisw synchronization for already-open terminals |
| B03 | Claude OAuth/keychain isolation and shared eligible Claude config, projects and history |
| B04 | Cursor CLI isolation using its credential file/token per invocation, without changing the user’s runtime HOME; opencode isolation through its profile XDG_DATA_HOME |
| B05 | Antigravity IDE profile/account switching, account avatar/duplicate recognition, and supported IDE quit/reopen flow |
| B06 | Claude/Codex API or proxy profiles, per-profile provider/key/model configuration and dedicated launchers |
| B07 | Local API gateway, local keys, account pools, model discovery/combos, fallback/cooldown and gateway usage tracking |
| B08 | Per-tool auto-switch for supported Claude/Codex quota windows, weekly locks and hidden-account exclusion across launchers, gateway and automatic actions |
| B09 | Usage and estimated cost reports, provider/all views, date ranges, account/model/project breakdowns, retained attribution for removed accounts and visible unknown/unpriced data |
| B10 | Settings/data compatibility, macOS menu-bar controls, remembered overlay geometry and existing saved accounts across upgrades |

Provider capabilities differ. Cursor/opencode do not automatically inherit Claude/Codex auto-switch, API/proxy or token-usage features. The destination must describe its actual provider support.

## Storage model to reproduce

Shared local session and configuration data:

- Main Codex home: normally ~/.codex; discover the actual configured path on the destination device.
- Local session rollouts, archived sessions, history, session index, project/sidebar metadata and a common SQLite catalog/history store.
- Eligible common config, rules, skills and memory/goals files where compatible. Keep databases with separate lifecycle/locking needs profile-local.

Private per-account data:

- Each provider/account has one canonical credential profile. Codex auth.json stays inside that profile.
- Switcher profiles live under its legacy macOS app-data directory, accounts/<provider>/<account-id>. Preserve compatible directory/keychain identifiers during upgrades.
- Provider-specific API/proxy keys/config remain private and separate.
- Exports and recovery checkpoints are separate private files, not public repo assets.

CLI and Desktop independently select a profile while pointing eligible local session storage to the shared catalog. Credentials and session storage have different ownership and migration rules. A future app may use other paths or supported APIs if it achieves the same behavior safely.

## Migration priorities

1. Preserve credentials and local history with private backups and an inventory.
2. Establish canonical profile ownership and resolve missing session catalogs, emails and stale quota.
3. Validate CLI and actual Desktop identity, then controlled handoff and recovery.
4. Carry over dropdown, overlay, glass interface, branding, Hello and export.
5. Validate supported baseline providers/features and install the intended release.

Before retiring the original app, report every index and baseline ID. Missing email/source/capabilities remain open items. Avoid promising complete conflict prevention or cross-account cloud history.

## Requirement history and repository evidence

This reference summarizes the owner's requested features and latest decisions. It preserves indices 1–19 from the original task list, adds older-session recovery as 20, maintained repository documentation as 21 and the JSON paste/parse dialog as 22. It summarizes requirements without publishing the private conversation transcript.

Use the current TASKS.md, README.md, CHANGELOG.md and implementation files as evidence of support. Associate a relevant file, release, validation result or remaining limitation with task status. Store machine-specific installation checks and private backup manifests outside public documentation when necessary.

A future app may use different paths, frameworks or supported APIs if it meets the same requirements. Provider and native backend capabilities must be checked against the installed version; this specification does not grant access to another account's cloud history.

## Reusable migration prompts

- [Audit a destination app](prompts/migration-audit.md): inspect support and produce the complete indexed gap list.
- [Implement an agreed migration](prompts/migration-implementation.md): carry the requirements into a chosen app and keep progress/documentation current.

Attach this reference to a destination agent, or give it the repository files. The audit prompt performs inspection; use the implementation prompt when implementation is requested.
