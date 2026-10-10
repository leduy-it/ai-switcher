# Michael Le Profiles

A macOS app for managing and switching accounts for Claude Code, Codex, and Antigravity.

## Install

- **DMG:** [Download the latest release](https://github.com/leduy-it/ai-switcher/releases/latest) and drag the app to Applications. On first launch, Control-click the app and choose **Open**; it is not signed or notarized.
- **Homebrew:** Available after the v0.15.0 release is published.

  ```sh
  brew tap leduy-it/ai-switcher https://github.com/leduy-it/ai-switcher.git
  brew install --cask leduy-it/ai-switcher/michael-le-profiles
  brew upgrade --cask --greedy leduy-it/ai-switcher/michael-le-profiles
  ```

## Features

- Manage separate accounts and switch between them for supported AI coding tools.
- View provider quota, account usage, and estimated costs.
- Use dedicated CLI accounts, a menu-bar quota view, and an optional floating overlay.
- Back up and append Codex or Claude Code credentials across Macs.
- Configure automatic account fallback and a local API gateway.

## More

- [User guide](docs/user-guide.md) — setup and feature details.
- [Changelog](CHANGELOG.md) — release-by-release changes.
- [Requirements and migration notes](docs/requirements.md)

The v0.15.0 source is on `main`; Homebrew installation requires its release asset to be published.
