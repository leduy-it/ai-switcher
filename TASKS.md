# Michael Le Profiles — requested task index

The app's owner requested these tasks in the development chat. Private prompts and credentials are
not reproduced here. This index tracks the requested features without publishing the chat transcript.

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
| 9 | Update version, build and install the macOS app | Version 0.13.0 built and installed |
| 10 | Native Liquid Glass, responsive UI, CSS and usage bars | Implemented |
| 11 | GitHub authentication, push and merge into the user's fork | Implemented; publication is tracked by release pull requests |
| 12 | List prior prompts and explain macOS refresh/install steps | Answered in the development chat |
| 13 | Dropdown table below the menu-bar icon; expand each account; full-screen UI | Implemented |
| 14 | Keep the floating overlay, improve its details and minimize like a pet | Implemented; draggable logo bubble and one-click expansion |
| 15 | Automate a small Hello for eligible Pro/Team/Business accounts | Implemented; app-local scheduler, existing tokens, durable cooldown |
| 16 | Export credentials by provider or all, raw Codex auth, email and usage | Implemented; private JSON export with unavailable-source notes |

Automatic Hello requires the app to remain running and the computer to be awake. It spends a small
amount of quota and only attempts a new five-hour window when live provider state allows it. Provider
errors, unknown email, and unattributed historical usage are not treated as successfully resolved data.
