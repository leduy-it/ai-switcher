# Desktop quota recovery repair implementation plan

> Execute sequentially using superpowers:executing-plans; review the complete change independently before integration.

**Goal:** Automatic quota fallback must reach the native desktop, load the selected managed credential and continue eligible original chats once, with usable status and controls.

**Architecture:** Retain Tauri/Rust recovery orchestration and native updater continuation. Use the installed desktop's supported local-daemon transport rather than an unrelated/private stdio server. Bind runtime ownership to the desktop's declared home, daemon process and verified socket; never infer success from selection alone.

**Spec:** docs/requirements.md indices 1–27, particularly 5, 10, 17–19, 25–26, plus the owner's follow-up to review earlier requests and repair UI.

## Constraints and review focus

- Preserve per-profile auth and shared history, import/export and native renewal; never log credentials.
- A pre-existing private desktop cannot be attached to an unrelated CLI daemon. Leave its active work intact; show the required one-time safe close and next managed launch clearly.
- Reject a dead/reused/wrong-profile daemon PID and unsupported native transport before mutation.
- Preserve unrelated CLI turns. Persist checkpoint/continuation intent before mutation; retries observe uncertain sends.
- Settings must remain cancellable while merely waiting; no-op waits must not rewrite state/broadcast continuously.
- Distinguish CLI selection, desktop identity and actual continuation. Do not mark a newer unrelated user turn as confirmed recovery.

## Task 1: Correct native managed desktop launch and ownership (final adapter design)

Files: src-tauri/src/desktop.rs, desktop_rpc.rs, models.rs, src/types.ts.
- [x] Add failing process/launch-contract tests covering ownership, transport delegation, pinned profile/catalog launch, private-backend precedence, Unix peer identity and default launch paths.
- [x] Implement the desktop-only stdio/Unix WebSocket adapter with preserved native CLI overrides, bounded lifecycle, private socket/record and verified GUI ownership. Delegate native startup/helper probes. Supersedes the rejected flag-only daemon launch.
- [x] Resolve the managed adapter child and compatible legacy native daemon clients; reject unrelated private/CLI clients and stop only an exclusive idle destination daemon.
- [x] Verify focused tests and the actual native GUI synthetic fixture with loopback inference, original-chat continuation, Rust ownership/cache/workspace verification and graceful child cleanup.

## Task 2: Repair quota fallback and recovery transitions

Files: src-tauri/src/app_state.rs, desktop_recovery.rs, desktop_state.rs.
- [x] Add failing tests for selected-CLI/desktop mismatch, exhausted desktop, no healthy account, pending interruption guards and observation-only continuation.
- [x] Use the desktop's last verified account when deciding desktop-first fallback; keep replacement eligibility provider-scoped, measured and healthy. Requeue the selected healthy destination if desktop still uses an exhausted account.
- [x] Keep durable recovery phases and pending markers authoritative. Avoid duplicate restart/send and unchanged waiting-state saves. Permit cancellation/settings changes only before interruption.
- [x] Run state/recovery tests plus existing isolated restart scenarios.

## Task 3: Repair status UI and audit delivery

Files: src/DesktopStatus.tsx, src/desktop-status.css, src/App.tsx, relevant styles; requirements/TASKS/README/changelog.
- [x] Show a compact desktop summary above accounts; move long controls/details into expansion. Keep actionable blocked/recovery status visible.
- [x] Disable impossible force-switch actions with an explicit reason; allow safe cancellation of a waiting operation. Fix header/action wrapping and narrow layouts.
- [x] Review original prompt requirements, record accurate support/limitations including private desktop bootstrap, and preserve requirement indices.
- [ ] Native installed UI wide/narrow inspection pending: Mac locked; owner asked to unlock. Rust/frontend/native fixture and independent logic review completed.
- [ ] Build/sign/install the local patch with account/credential inventory preserved; push verified changes to main and report actual native validation limits.

## Execution ledger

- Ruling: work in the existing clean checkout and integrate directly into main, consistent with the owner's sequential/main delivery request; no new worktree or implementation agents.
- Native package research: the installed desktop uses a direct Unix WebSocket daemon client when CODEX_APP_SERVER_USE_LOCAL_DAEMON=1 and no alternate/forced CLI transport is configured. There is no child proxy in that mode. Native daemon start installs a managed release under the profile; executable ownership cannot be equated to the original app bundle path.
- Ruling: use an owner-only short directory alias for CODEX_HOME to satisfy macOS's socket path limit; auth.json itself remains an ordinary private file in the canonical account profile. Canonicalize observed paths before comparison.
- Task 1 red/green: four launch/ownership/alias tests initially failed to compile before implementation, then passed. Added private-child precedence and actual socket peer-PID tests. Socketpair does not establish macOS LOCAL_PEERPID; replaced it with a real Unix listener/connect fixture.
- Ruling after actual native GUI validation: the environment-only local-daemon launch is insufficient. Native getConfigOverrides is nonempty for real desktop tools/auth, so its transport intentionally falls back to stdio. The initial native GUI fixture consumed the marker and used B, but a second controller could not observe its new turn because it was a separate private backend. Do not ship the flag-only approach.
- Native CLI probes rejected multiple --listen values and --stdio with --listen. Replace the launch architecture with a private local stdio-to-Unix-WebSocket bridge: preserve every native CLI override/tool runtime, start exactly one owned native backend with --listen unix://SHORT_SOCKET, forward native messages transparently, and expose that SAME backend to the manager. Dedicated CODEX_CLI_PATH override is scoped to this desktop only; global CLI launchers remain untouched. Record peer PID/frontend/child ownership privately. Stop only a proven idle destination daemon; never compete with an active native auth owner.
- Independent review found P1 preflight-target drift, P1 stale native resume candidate after user activity, P2 hard provider-error classification loss and P2 repeated selection writes. Fix these with focused regressions before build/install.
- Latest baseline before bridge replacement: full Rust suite 176 passed / 2 ignored; frontend build + 7 UI state scenarios pass; 8 isolated native subprocess scenarios pass. Actual GUI resumed in a separate stdio core, so full managed-GUI validation is still outstanding.

- Final transport ruling supersedes the earlier flag/alias proposal: use a private per-profile/catalog/native/executable wrapper, pin both storage variables after native shell initialization, and bridge the native GUI's own CLI/tool arguments to its one private Unix WebSocket child. No auth-home alias is required in production. Native startup policy/version/helper commands delegate untouched.
- Native GUI fixture caught shell hook replacement of CODEX_SQLITE_HOME. Five synthetic fixture rows had entered the default catalog during prototype checks; made a consistent private backup and removed only those exact fixture IDs with matching synthetic cwd/provider/rollout. The corrected wrapper fixture verifies the private catalog, GUI/proxy/backend chain, socket peer and cached credential/workspace, with one completed A→B continuation preserving thread/model/cwd. Graceful GUI termination removed record and child.
- Final review covered native queued/unqueued retries, same-turn quota failure between checkpoint and interruption, five-second child cleanup before the native eight-second deadline, unrelated CLI proxies, last-turn user activity, saved destination and repeated selection writes. Added regression coverage for these boundaries.

- Final verification: full suite 182 passed / 3 opt-in ignored; isolated native daemon and actual native GUI ownership tests passed separately; frontend build + 7 UI-state scenarios and 8 native subprocess scenarios pass. Signed release arm64 bundle 0.16.1 native GUI check passed. Installation/relaunch and wide/narrow native visual inspection remain blocked by locked Mac; owner requested to unlock. Installed app remains 0.16.0.

- Later delivery during 0.16.2: UI connectivity recovered. Installed/running 0.16.2 includes this follow-up; compact native layout/modal inspected. Existing 12 accounts/19 credential/config files preserved. Exact minimum-width resize remains unconfirmed; original private desktop still waits for one safe close.
