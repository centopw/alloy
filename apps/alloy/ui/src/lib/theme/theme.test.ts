import { describe, it, expect, beforeEach, vi } from "vitest";
import { themeStore, loadCustomTheme } from "./theme.svelte";

describe("ThemeStore", () => {
  beforeEach(() => {
    localStorage.clear();
    themeStore.setDensity("normal");
    themeStore.setMessageStyle("bubble");
    themeStore.setTheme("alloy-dark");
    themeStore.setFollowSystem(true);
    themeStore.applyToDOM();
    const customStyle = document.getElementById("alloy-user-theme");
    if (customStyle) customStyle.remove();
  });

  it("initializes with default values and applies attributes to html", () => {
    expect(themeStore.activeTheme).toBeDefined();
    expect(themeStore.density).toBe("normal");
    expect(themeStore.messageStyle).toBe("bubble");

    // Attributes on <html>
    expect(document.documentElement.getAttribute("data-density")).toBe("normal");
    expect(document.documentElement.getAttribute("data-message-style")).toBe("bubble");
  });

  it("persists theme change to localStorage and updates DOM", () => {
    themeStore.setTheme("alloy-light");

    expect(themeStore.activeTheme).toBe("alloy-light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("alloy-light");

    // Verify localStorage
    const stored = JSON.parse(localStorage.getItem("alloy.theme.v1") || "{}");
    expect(stored.theme).toBe("alloy-light");
  });

  it("persists density change to localStorage and updates DOM", () => {
    themeStore.setDensity("compact");

    expect(themeStore.density).toBe("compact");
    expect(document.documentElement.getAttribute("data-density")).toBe("compact");

    const stored = JSON.parse(localStorage.getItem("alloy.theme.v1") || "{}");
    expect(stored.density).toBe("compact");

    themeStore.setDensity("cozy");
    expect(themeStore.density).toBe("cozy");
    expect(document.documentElement.getAttribute("data-density")).toBe("cozy");
  });

  it("persists message style change to localStorage and updates DOM", () => {
    themeStore.setMessageStyle("gutter");

    expect(themeStore.messageStyle).toBe("gutter");
    expect(document.documentElement.getAttribute("data-message-style")).toBe("gutter");

    const stored = JSON.parse(localStorage.getItem("alloy.theme.v1") || "{}");
    expect(stored.messageStyle).toBe("gutter");

    themeStore.setMessageStyle("bubble");
    expect(themeStore.messageStyle).toBe("bubble");
    expect(document.documentElement.getAttribute("data-message-style")).toBe("bubble");
  });

  it("cycles through light, dark, and amoled themes", () => {
    themeStore.setTheme("alloy-light");
    themeStore.cycleTheme();
    expect(themeStore.activeTheme).toBe("alloy-dark");

    themeStore.cycleTheme();
    expect(themeStore.activeTheme).toBe("alloy-amoled");

    themeStore.cycleTheme();
    expect(themeStore.activeTheme).toBe("alloy-light");
  });

  it("supports followSystem setting", () => {
    themeStore.setFollowSystem(false);
    expect(themeStore.followSystem).toBe(false);

    const stored = JSON.parse(localStorage.getItem("alloy.theme.v1") || "{}");
    expect(stored.followSystem).toBe(false);

    themeStore.setFollowSystem(true);
    expect(themeStore.followSystem).toBe(true);
  });

  it("injects and replaces custom theme CSS via loadCustomTheme", () => {
    const customCss1 = ":root { --alloy-color-accent-primary: #ff00ff; }";
    loadCustomTheme(customCss1);

    let styleEl = document.getElementById("alloy-user-theme");
    expect(styleEl).not.toBeNull();
    expect(styleEl?.textContent).toBe(customCss1);

    const customCss2 = ":root { --alloy-color-accent-primary: #00ffff; }";
    loadCustomTheme(customCss2);

    const allCustomStyles = document.querySelectorAll("#alloy-user-theme");
    expect(allCustomStyles.length).toBe(1);
    expect(allCustomStyles[0].textContent).toBe(customCss2);
  });
});
