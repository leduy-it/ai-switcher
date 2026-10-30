# Same-profile Codex auth import

Feature: explicit in-place Codex auth import, separate from append-only add/list import. Card action -> metadata preview (current/new email, opaque hash bound to source+current credential+target ID) -> import. Reject other-profile duplicates and live owners/pending recovery. Serialize with existing import/switch lock and the same per-profile native-renewal mutex. Private backup + atomic auth replacement + metadata-save rollback; preserve ID/name/launcher/selection/config/history/locks, clear stale email/quota/desktop proof for changed auth. Tests use synthetic temp profiles only. No production credential is replaced during development. Install/UI QA remain contingent on unlocked Mac.

Execution ledger:
- Added explicit per-card import, local metadata preview and source/current-auth/target-ID fingerprint. Existing Add/list flows keep appending.
- Added private atomic credential transaction, backup, rollback and live owner/recovery/duplicate guards. Shared renewal mutex prevents this app's competing auth write.
- Independent review found stale deferred weekly locks and old derived lock retention. Fixed revision-bound serialized effects, reset derived lock with reserve settings retained, restore old locked launcher or return a separate warning.
- Preserve known old/new creator labels in a metadata-only registry for Usage; current native identities remain authoritative.
- Tests use temporary synthetic profiles; real auth is not replaced in development. Installed UI preview will use fake JSON and cancel before import.

Validation complete: 191 Rust tests passed / 3 opt-in ignored; frontend build and 7 UI state scenarios passed. Independent review found no further blockers after fixes. Installed/signed/relaunched 0.16.2, preserving 12 account IDs and 19 credential/config hashes. Installed native UI fake preview showed old/new email, then cancelled without replacing auth. Normal-width layout/modal checked; exact narrow-window resize unconfirmed. Original private desktop left running.
