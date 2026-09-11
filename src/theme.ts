import type { Theme } from "./api";

export const THEMES: [Theme, string][] = [
  ["charcoal", "Charcoal"],
  ["midnight", "Midnight"],
  ["paper", "Paper"],
  ["mist", "Mist"],
];

const CACHE_KEY = "appearance-theme";

/** The backend is authoritative; this cache supplies the colour before IPC returns. */
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(CACHE_KEY, theme);
  } catch {
    // Storage can be unavailable; the persisted backend preference still works.
  }
}

export function restoreTheme(): void {
  let cached: string | null = null;
  try {
    cached = localStorage.getItem(CACHE_KEY);
  } catch {
    // First paint uses Charcoal until the backend supplies the preference.
  }
  applyTheme(THEMES.find(([id]) => id === cached)?.[0] ?? "charcoal");
}
