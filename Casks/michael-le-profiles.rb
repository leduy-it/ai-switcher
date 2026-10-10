cask "michael-le-profiles" do
  version :latest
  sha256 :no_check

  url "https://github.com/leduy-it/ai-switcher/releases/latest/download/michael-le-profiles.dmg"
  name "Michael Le Profiles"
  desc "Manage local accounts for AI coding tools"
  homepage "https://github.com/leduy-it/ai-switcher"

  app "Michael Le Profiles.app"

  caveats <<~EOS
    This app is not code-signed or notarized. On first launch, Control-click it in Applications
    and choose Open.

    Upgrade to the latest release with:
      brew upgrade --cask --greedy leduy-it/ai-switcher/michael-le-profiles
  EOS
end
