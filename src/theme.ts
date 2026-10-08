export type ProfileTheme = "original" | "michael-blue";

const STORAGE_KEY = "ai-account-switcher.profile-theme";

export function readProfileTheme(): ProfileTheme {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "michael-blue"
      ? "michael-blue"
      : "original";
  } catch {
    return "original";
  }
}

export function applyProfileTheme(theme: ProfileTheme) {
  document.documentElement.dataset.profileTheme = theme;
  document.title =
    theme === "michael-blue" ? "Michael Le (duyle) Profiles" : "AI Account Switcher";
}

export function saveProfileTheme(theme: ProfileTheme) {
  applyProfileTheme(theme);
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // The current window still receives the selected theme if storage is unavailable.
  }
}
