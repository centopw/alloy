/**
 * Theme management system for Alloy.
 *
 * Features:
 * - Reactive runes store: activeTheme, density, messageStyle, followSystem
 * - localStorage persistence (alloy.theme.v1)
 * - System preference tracking (prefers-color-scheme, prefers-contrast, prefers-reduced-motion)
 * - Custom theme injection via loadCustomTheme(css)
 * - Automatic <html> attribute application
 *
 * Usage:
 *   import { activeTheme, density, messageStyle, followSystem } from '$lib/theme/theme.svelte';
 *
 *   // In a Svelte component
 *   <script>
 *     import * as themeStore from '$lib/theme/theme.svelte';
 *
 *     $effect(() => {
 *       console.log("Current theme:", themeStore.activeTheme.current);
 *     });
 *   </script>
 */

export type Theme = "alloy-light" | "alloy-dark" | "alloy-amoled";
export type Density = "compact" | "normal" | "cozy";
export type MessageStyle = "bubble" | "gutter";

const STORAGE_KEY = "alloy.theme.v1";

// Preferences from system
type SystemPreferences = {
  colorScheme: "light" | "dark";
  contrastPreference: "no-preference" | "less" | "more";
  prefersReducedMotion: boolean;
};

function getSystemPreferences(): SystemPreferences {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return {
      colorScheme: "dark",
      contrastPreference: "no-preference",
      prefersReducedMotion: false,
    };
  }

  return {
    colorScheme: window.matchMedia("(prefers-color-scheme: light)").matches
      ? "light"
      : "dark",
    contrastPreference: window.matchMedia("(prefers-contrast: more)").matches
      ? "more"
      : window.matchMedia("(prefers-contrast: less)").matches
        ? "less"
        : "no-preference",
    prefersReducedMotion: window.matchMedia("(prefers-reduced-motion: reduce)")
      .matches,
  };
}

// Persisted preferences
interface StoredPreferences {
  theme?: Theme;
  density?: Density;
  messageStyle?: MessageStyle;
  followSystem?: boolean;
}

function loadStoredPreferences(): StoredPreferences {
  if (typeof window === "undefined") {
    return {};
  }

  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      return JSON.parse(stored) as StoredPreferences;
    }
  } catch (e) {
    console.warn("Failed to load theme preferences from localStorage", e);
  }
  return {};
}

function saveStoredPreferences(prefs: StoredPreferences): void {
  if (typeof window === "undefined") {
    return;
  }

  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
  } catch (e) {
    console.warn("Failed to save theme preferences to localStorage", e);
  }
}

// Reactive state via runes
class ThemeStore {
  activeTheme = $state<Theme>("alloy-dark");
  density = $state<Density>("normal");
  messageStyle = $state<MessageStyle>("bubble");
  followSystem = $state<boolean>(true);
  systemPreferences = $state<SystemPreferences>(getSystemPreferences());

  constructor() {
    // Load from localStorage
    const stored = loadStoredPreferences();
    if (stored.theme) this.activeTheme = stored.theme;
    if (stored.density) this.density = stored.density;
    if (stored.messageStyle) this.messageStyle = stored.messageStyle;
    if (stored.followSystem !== undefined) this.followSystem = stored.followSystem;

    // Initialize in browser
    if (typeof window !== "undefined") {
      this.applyToDOM();
      this.setupListeners();
    }
  }

  applyToDOM(): void {
    if (typeof document === "undefined") return;

    const html = document.documentElement;
    html.setAttribute("data-theme", this.activeTheme);
    html.setAttribute("data-density", this.density);
    html.setAttribute("data-message-style", this.messageStyle);
    html.setAttribute("data-reduced-motion", this.systemPreferences.prefersReducedMotion ? "true" : "false");
  }

  private setupListeners(): void {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;

    // Watch system color-scheme preference
    const colorSchemeQuery = window.matchMedia("(prefers-color-scheme: light)");
    colorSchemeQuery.addEventListener("change", (e) => {
      this.systemPreferences.colorScheme = e.matches ? "light" : "dark";
      if (this.followSystem) {
        this.setTheme(this.systemPreferences.colorScheme === "light" ? "alloy-light" : "alloy-dark");
      }
    });

    // Watch contrast preference
    const contrastMoreQuery = window.matchMedia("(prefers-contrast: more)");
    const contrastLessQuery = window.matchMedia("(prefers-contrast: less)");
    contrastMoreQuery.addEventListener("change", () => {
      this.systemPreferences.contrastPreference = "more";
    });
    contrastLessQuery.addEventListener("change", () => {
      this.systemPreferences.contrastPreference = "less";
    });

    // Watch reduced-motion preference
    const reducedMotionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    reducedMotionQuery.addEventListener("change", (e) => {
      this.systemPreferences.prefersReducedMotion = e.matches;
      if (typeof document !== "undefined") {
        document.documentElement.setAttribute(
          "data-reduced-motion",
          e.matches ? "true" : "false",
        );
      }
    });
  }

  setTheme(theme: Theme): void {
    this.activeTheme = theme;
    if (typeof document !== "undefined") {
      document.documentElement.setAttribute("data-theme", theme);
    }
    const stored = loadStoredPreferences();
    saveStoredPreferences({ ...stored, theme });
  }

  setDensity(density: Density): void {
    this.density = density;
    if (typeof document !== "undefined") {
      document.documentElement.setAttribute("data-density", density);
    }
    const stored = loadStoredPreferences();
    saveStoredPreferences({ ...stored, density });
  }

  setMessageStyle(style: MessageStyle): void {
    this.messageStyle = style;
    if (typeof document !== "undefined") {
      document.documentElement.setAttribute("data-message-style", style);
    }
    const stored = loadStoredPreferences();
    saveStoredPreferences({ ...stored, messageStyle: style });
  }

  setFollowSystem(follow: boolean): void {
    this.followSystem = follow;
    const stored = loadStoredPreferences();
    saveStoredPreferences({ ...stored, followSystem: follow });
  }

  cycleTheme(): void {
    const themes: Theme[] = ["alloy-light", "alloy-dark", "alloy-amoled"];
    const currentIndex = themes.indexOf(this.activeTheme);
    const nextTheme = themes[(currentIndex + 1) % themes.length];
    this.setTheme(nextTheme);
  }

  /**
   * Load and inject a custom user theme CSS.
   * Injects a <style id="alloy-user-theme"> element into the document head.
   * This is used when the engine returns a user-defined theme CSS.
   *
   * @param css - Raw CSS text containing --alloy-* variables
   */
  loadCustomTheme(css: string): void {
    if (typeof document === "undefined") return;

    // Remove existing custom theme if any
    const existing = document.getElementById("alloy-user-theme");
    if (existing) {
      existing.remove();
    }

    // Insert new custom theme style
    const style = document.createElement("style");
    style.id = "alloy-user-theme";
    style.textContent = css;
    document.head.appendChild(style);
  }
}

// Export singleton instance
export const themeStore = new ThemeStore();

/**
 * Standalone API to load and inject custom user theme CSS.
 * Injects a <style id="alloy-user-theme"> element into the document head.
 * This is used when the engine returns a user-defined theme CSS.
 *
 * @param css - Raw CSS text containing --alloy-* variables
 */
export function loadCustomTheme(css: string): void {
  if (typeof document === "undefined") return;

  const existing = document.getElementById("alloy-user-theme");
  if (existing) {
    existing.remove();
  }
  const style = document.createElement("style");
  style.id = "alloy-user-theme";
  style.textContent = css;
  document.head.appendChild(style);
}
