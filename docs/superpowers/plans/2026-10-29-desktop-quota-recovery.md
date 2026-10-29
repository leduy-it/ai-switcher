# Desktop-first quota recovery

User requests sequential tasks; native app updater restoration is a design reference.

- [x] JSON sharing committed as 3b11dac, reviewed; full suite 134 passed, 1 ignored, frontend build passed.
- [x] Reproduce provider-default lookup returning Claude for Codex; hidden active account unidentifiable; unmeasured replacement treated as healthy. Regression tests fail, then pass after scoped fixes. Suite: 137 passed, 1 live-auth test ignored.
- [ ] Native desktop reload/continuation: source updater saves accountId/userId/threadIds/expiry under update-resume-state-v1 and native UI starts an empty continuation turn. Capture only known eligible threads, preserve client-owned tool runtime, verify identity and avoid duplicate resend. Private stdio backends require explicit capability handling.
- [ ] Review, run isolated restart/identity scenarios and source tests/build, merge all fork branch histories, push main and verify remote SHA.

## Sources
Official protocol: https://developers.openai.com/codex/app-server
Installed desktop source inspected read-only in app.asar; no active app stopped.
