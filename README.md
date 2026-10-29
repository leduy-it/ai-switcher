# Michael Le Profiles

A macOS app for managing and switching accounts for Claude Code, Codex, and Antigravity.

## Install

- **DMG:** [Download the latest release](https://github.com/leduy-it/ai-switcher/releases/latest) and drag the app to Applications. On first launch, Control-click the app and choose **Open**; it is not signed or notarized.
- **Homebrew:**

  ```sh
  brew tap leduy-it/ai-switcher https://github.com/leduy-it/ai-switcher.git
  brew install --cask leduy-it/ai-switcher/michael-le-profiles
  brew upgrade --cask --greedy leduy-it/ai-switcher/michael-le-profiles
  ```

## Features

- Manage separate accounts and switch between them for supported AI coding tools.
- View provider quota, account usage, and estimated costs. Codex credential imports handle rich-text clipboard spacing and show actionable sign-in errors when live quota is unavailable.
- Use dedicated CLI accounts, a menu-bar quota view, and an optional floating overlay with CPU/RAM and busiest-app details.
- Share Codex OAuth accounts as native auth.json files or one JSON array; preview and select accounts for bulk import. Legacy provider/all backups also support Claude Code.
- Find and reopen local Codex chats across profiles while desktop work continues.
- Configure automatic account fallback and a local API gateway.

## More

- [User guide](docs/user-guide.md) — setup and feature details.
- [Changelog](CHANGELOG.md) — release-by-release changes.
- [Requirements and migration notes](docs/requirements.md)

[MIT license](LICENSE) · Original project © 2026 Hoang Phan.
