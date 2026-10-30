# Migration implementation prompt

Read [the requirements](../requirements.md), [the task index](../../TASKS.md) and the completed audit. Replace the target placeholder. Use this prompt when implementation in the chosen destination is requested.

Implement the agreed migration for [TARGET_APP_OR_REPO] using the current docs/requirements.md, TASKS.md and the completed audit, or their attached copies.

First present the full task list for EVERY requirement and baseline ID in the current specification, with current statuses and planned gaps. Keep the original numbering. Resolve routine implementation choices from the requirements and repository instructions, and keep changes limited to the chosen target.

Preserve existing user data and credential isolation. Discover actual storage/profile/desktop paths and build an additive, schema-aware history migration with private consistent backups and a manifest. Consolidate managed duplicate logins into one canonical profile without deleting original data. Keep creator email and live quota tied to the correct identity.

Preserve desktop visibility independently of backend history. Recover projectless chat IDs and
workspace/project placement records, including legacy backups. Use regular state snapshots after
safe desktop exit rather than symlinks replaced by atomic saves. Refresh only the incoming local
desktop catalog scan when restored timestamps predate its watermark; preserve other hosts and
automations. Verify visibility separately from database import counts.

Provide a metadata-only local chat finder independently of migration and handoff. It must remain
usable while a desktop is busy or its private transport cannot be controlled. Reopen an existing
shared-store chat by its original ID without sending a prompt or restarting its backend; identify
profile-only history for later idle repair. Preserve shared archive state when deduplicating results.

Resolve the desktop-owned backend before inspecting identity or active work. Never substitute a
separate CLI daemon for a private desktop connection. Validate credential restart and same-thread
continuation in isolated subprocesses with synthetic credentials and a loopback provider; report
native desktop-owned tool recovery separately from backend fixture coverage.

Implement the agreed supported CLI/Desktop, safe handoff/recovery, UI, Hello, export, Codex JSON paste/parse, provider backup import and DMG/Homebrew install requirements. Capability-limited features must expose an accurate need-for-attention state rather than imitate success. Retain the latest Michael Le blue Liquid Glass identity and docs attribution.

Maintain a requirement-to-change log with evidence and remaining limitations. Establish a living requirements document, task index and reusable prompts in the destination repository. Add an AGENTS.md rule requiring their updates in the same change as every relevant feature, behavior, provider, UI, credential/session storage, migration or release change. Append new indices and retain superseded ones. Update README/changelog statements when affected; review docs against final behavior before reporting completion. Separate source implementation, build, installed app validation and published release status. Do not add or run tests unless I request testing; explain which authorized validation is still needed.

This prompt authorizes target implementation, not token use or disruption of active work. Before any real account handoff, app/session interruption, Hello send, recovery continuation, credential export/transfer, deletion or external publication, check the explicit authorization in this new migration conversation. Do not inherit permissions from the old development chat or from this document. Prepare the concrete change before requesting any missing action-specific approval.

When finished, return the complete indexed status table, changed files/artifact, private backup/rollback locations, intended versus installed version and any unsupported/unverified behavior. Never publish secrets or private session bodies.

For Codex auth imports, verify Unicode clipboard whitespace repair outside quoted strings, exact credential preservation, unchanged source files and metadata-only previews. Check that live quota includes the selected workspace and that rejected credentials remain an actionable sign-in error; parsing/import must not be described as successful live authentication or token renewal.

For native Codex JSON sharing, preserve object/array structure, selectable bulk import and file-fingerprint validation. For requirements 25–26, validate guarded native desktop continuation and native auth-only renewal separately. Do not treat access expiry as permanent logout; verify unchanged workspace/user and positive new access expiry. Preserve active desktop/CLI ownership, process deadlines and secret isolation. Never claim real-provider renewal/native UI recovery from fixture-only checks.

For managed native desktop launch, preserve startup policy and every native CLI/tool override. Verify that the manager controls the SAME GUI backend; environment-only daemon opt-in can silently fall back to stdio. Verify private adapter ownership, Unix socket peer, graceful native GUI SIGTERM cleanup, exclusive destination ownership, saved-target revalidation and the original last turn before one-shot continuation. Keep quota-failed recovery scoped to recent explicit usageLimitExceeded, never completed/stopped/old failures. Validate compact status/settings and narrow export/header actions.

For requirement 28, keep explicit same-profile auth replacement separate from append-only list import. Validate stable ID/name/command/selection/history, source+target preview binding, other-profile duplicates, live owner/recovery guards, shared renewal locking, private backup/rollback and credential-revision checks for quota assignment and deferred locks/notifications. Preserve reserve settings while discarding old login-derived lock state.
