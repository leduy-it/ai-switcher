export type ProfileTheme = "michael-blue";

export function readProfileTheme(): ProfileTheme {
  return "michael-blue";
}

export function applyProfileTheme(_theme: ProfileTheme = "michael-blue") {
  document.documentElement.dataset.profileTheme = "michael-blue";
  document.title = "Michael Le Profiles";
}
