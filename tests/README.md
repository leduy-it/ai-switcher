# Isolated desktop-switch scenarios

Run the subprocess check against an installed Codex binary:

```sh
python3 tests/desktop_switch_scenarios.py --codex /absolute/path/to/codex
```

The harness creates private temporary profile homes, a shared test catalog, synthetic OAuth
credentials and a local HTTP/SSE provider. It starts and stops only its own backend subprocesses.
It does not open or quit a desktop app, read real credentials, modify main profile storage or spend
provider quota. A successful run saves `report.json` beside its private fixture logs.

Checks: identity A, active-turn interruption, restart under B, original-thread resume, inference
using B, persisted acknowledgement on recovery retry, and unchanged credential files. The native
desktop UI and desktop-owned dynamic tool runtime are outside this harness's coverage.

Backend discovery and controller ownership regressions run with:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml desktop --lib
```
