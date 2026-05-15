import { useEffect, useState } from "react";

/**
 * Theme handling. Three states:
 *   - `"dark"` / `"light"` — manual override; written to localStorage,
 *     mirrored to `<html data-theme>`. Wins over the OS preference.
 *   - `"system"` — clear the override; CSS falls back to
 *     `@media (prefers-color-scheme: light)`.
 *
 * The CSS cascade is set up by `ui/scripts/render-tokens.mjs` —
 * see `ui/src/index.css` after generation. This module only flips the
 * attribute; all colour resolution happens in CSS.
 *
 * To avoid FOUC, the initial value is also written by an inline script
 * in `ui/index.html` *before* React mounts.
 */

export type ThemeChoice = "dark" | "light" | "system";

export const THEME_STORAGE_KEY = "aegis-theme";

/** Read the persisted choice. Defaults to `"system"` if absent or invalid. */
export function readStoredTheme(): ThemeChoice {
  if (typeof window === "undefined") return "system";
  try {
    const v = window.localStorage.getItem(THEME_STORAGE_KEY);
    if (v === "dark" || v === "light") return v;
  } catch {
    // localStorage may throw under privacy modes / sandboxed iframes —
    // not a fatal condition; fall through to default.
  }
  return "system";
}

/** Apply a choice to <html> and localStorage. */
export function applyTheme(choice: ThemeChoice): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  if (choice === "system") {
    root.removeAttribute("data-theme");
    try {
      window.localStorage.removeItem(THEME_STORAGE_KEY);
    } catch {
      // ignore — see readStoredTheme
    }
  } else {
    root.setAttribute("data-theme", choice);
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, choice);
    } catch {
      // ignore — see readStoredTheme
    }
  }
}

/**
 * Reactive hook returning the current choice + the effective mode.
 *   - `choice` is what the user picked (or "system" by default).
 *   - `effective` is the actual mode in use right now ("dark" or
 *     "light") — useful for icons / aria-pressed.
 *
 * Listens to `prefers-color-scheme` so a system-level switch
 * propagates without manual override.
 */
export function useTheme(): {
  choice: ThemeChoice;
  effective: "dark" | "light";
  setChoice: (c: ThemeChoice) => void;
  cycle: () => void;
} {
  const [choice, setChoiceState] = useState<ThemeChoice>(() =>
    readStoredTheme(),
  );
  const [systemPrefersLight, setSystemPrefersLight] = useState<boolean>(() => {
    if (typeof window === "undefined" || !window.matchMedia) return false;
    return window.matchMedia("(prefers-color-scheme: light)").matches;
  });

  useEffect(() => {
    if (!window.matchMedia) return;
    const mq = window.matchMedia("(prefers-color-scheme: light)");
    const onChange = (e: MediaQueryListEvent) =>
      setSystemPrefersLight(e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  const setChoice = (c: ThemeChoice) => {
    applyTheme(c);
    setChoiceState(c);
  };

  // 2-state cycle: dark ↔ light. Explicit "system" reset is not part
  // of the chrome — users can clear localStorage to get it back. Keep
  // the toggle simple; an explicit menu lands when we have more themes.
  const cycle = () => {
    const next: ThemeChoice =
      choice === "light" ? "dark" : choice === "dark" ? "light" : systemPrefersLight ? "dark" : "light";
    setChoice(next);
  };

  const effective: "dark" | "light" =
    choice === "system" ? (systemPrefersLight ? "light" : "dark") : choice;

  return { choice, effective, setChoice, cycle };
}
