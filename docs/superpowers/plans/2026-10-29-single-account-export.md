# Codex native auth sharing implementation

**Goal:** Export one native Codex auth object or all as an array, with selectable bulk import.
**Spec:** ../specs/2026-10-29-single-account-export-design.md

- [x] Native object/array export in Rust, exact field preservation, private atomic writes; skip invalid/API bulk entries and fail stale single-account IDs.
- [x] Native object/array import with format validation, metadata-only preview, per-account selection and file fingerprint check; preserve legacy backup import.
- [x] Add card export and Codex-section bulk import; update guide, requirements and task status.
- [x] Independent review found native BOM/Unicode parsing and existing token-only dedupe gaps; add regression tests and fixes. Single-read export and accurate skip wording also addressed.
- [x] Rust suite: 134 passed, 1 authorized-live test ignored. Frontend production build passed.
- [ ] Commit; later integrate remote histories and push main after desktop tasks.

## Review focus
Cross-account leakage; stale/mismatched IDs; hidden/default OAuth; API exclusion; missing source reporting; changed-file selection; native and legacy importer interoperability.
