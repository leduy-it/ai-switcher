# Codex native JSON sharing

Final user clarification: individual export is a native Codex auth.json object. Export all is one JSON array of native auth.json objects, without backup wrappers, app metadata, config, quota or history. Import all accepts that array, previews metadata only and allows selecting/deselecting accounts. Existing single-account import and legacy provider backups remain supported. The user requests sequential implementation followed by logic review and main integration.

Use the existing Rust private atomic writer. OAuth only for native exports; API/proxy backups retain their existing provider/all format because auth.json alone cannot represent their gateway settings. Skip unavailable sources in bulk with explicit counts; fail a single missing source. Preserve single auth file bytes and all JSON field values in arrays. Native import validates every entry and reports invalid entries without leaking secrets. Reuse additive profile import and duplicate checks; never overwrite local profiles.

## Approaches
Chosen native object/array matches Codex structures and permits straightforward manual add/remove. Prior schema-v1 wrapper proposal was rejected by the user. Keep legacy backups readable for existing users.
