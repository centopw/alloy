# alloy-theme

Design token engine, theme loader, and CSS custom property generator for the **Alloy** Matrix client.

`alloy-theme` provides a strongly typed, zero-unsafe Rust foundation for Tier-A declarative theme packages, enforcing W3C Design Tokens conventions, compile-time/parse-time schema validation, WCAG 2.1 contrast compliance, and deterministic CSS variable generation.

---

## 1. Architecture Overview

Themes in Alloy follow a declarative JSON/JSONC schema (comments and trailing commas permitted). Themes can either declare a complete token palette or specify `extends: "<base-theme-id>"` to inherit and deep-merge tokens from a built-in base theme (`alloy-light`, `alloy-dark`, `alloy-amoled`).

```text
┌────────────────────────────────┐
│  Theme File (.json / .jsonc)   │
│  - metadata (id, mode, etc.)   │
│  - extends?: "alloy-dark"      │
│  - tokens { color, typo, ... } │
│  - overrides? { token: val }   │
└───────────────┬────────────────┘
                │ ThemeLoader (jsonc-parser)
                ▼
┌────────────────────────────────┐
│       Theme Definition         │
│  (Strict serde validation,     │
│   unknown keys rejected)       │
└───────────────┬────────────────┘
                │ ThemeResolver (deep merge)
                ▼
┌────────────────────────────────┐
│        ResolvedTheme           │
│  - Flat token path -> value    │
│  - WCAG ContrastReport         │
│  - to_css_vars()               │
└───────────────┬────────────────┘
                │ CSS Contract
                ▼
┌────────────────────────────────┐
│  Platform Webview / UI Shell   │
│  (:root { --alloy-*: ...; })   │
└────────────────────────────────┘
```

---

## 2. Token Naming Scheme & CSS Variable Contract

The UI layer (Svelte 5 components, Tailwind config, and CSS stylesheets) consumes tokens via standard CSS custom properties.

### Mapping Rule
A dot-separated token path `category.group.token_name` maps deterministically to:
$$\text{token path} \longrightarrow \texttt{--alloy-}\{\text{kebab-cased path}\}$$

All dots (`.`) and underscores (`_`) become hyphens (`-`). All names are lowercase.

### Stable CSS Variables Reference

#### Color Tokens (`--alloy-color-*`)
| Token Path | CSS Custom Property | Description |
| :--- | :--- | :--- |
| `color.surface.canvas` | `--alloy-color-surface-canvas` | Main window and timeline background |
| `color.surface.subtle` | `--alloy-color-surface-subtle` | Sidebar, room list, secondary panes |
| `color.surface.panel` | `--alloy-color-surface-panel` | Modals, inspector cards, dropdown menus |
| `color.surface.elevated` | `--alloy-color-surface-elevated`| Tooltips, popovers, floating menus |
| `color.surface.hover` | `--alloy-color-surface-hover` | Hover state background on surfaces |
| `color.background` | `--alloy-color-background` | App root canvas background |
| `color.text.primary` | `--alloy-color-text-primary` | High-contrast body text (min 4.5:1 AA) |
| `color.text.secondary` | `--alloy-color-text-secondary` | Timestamps, room subtitle, room descriptions |
| `color.text.muted` | `--alloy-color-text-muted` | Placeholder text, disabled controls |
| `color.text.link` | `--alloy-color-text-link` | Interactive links and mentions |
| `color.text.on_accent` | `--alloy-color-text-on-accent` | Text rendered on top of `accent.primary` |
| `color.accent.primary` | `--alloy-color-accent-primary` | Brand primary color (Default: `#5865F2` Indigo) |
| `color.accent.secondary` | `--alloy-color-accent-secondary` | Secondary brand action tint |
| `color.accent.subtle` | `--alloy-color-accent-subtle` | Light tint for selections, badges, highlights |
| `color.accent.hover` | `--alloy-color-accent-hover` | Hover state for accent buttons |
| `color.status.online` | `--alloy-color-status-online` | User online presence indicator (Green) |
| `color.status.away` | `--alloy-color-status-away` | Idle/away presence indicator (Amber) |
| `color.status.busy` | `--alloy-color-status-busy` | Do Not Disturb indicator (Red) |
| `color.status.offline` | `--alloy-color-status-offline` | Offline user indicator (Gray) |
| `color.status.error` | `--alloy-color-status-error` | Send failure, error notices |
| `color.status.warning` | `--alloy-color-status-warning` | Security warnings, unverified session alerts |
| `color.status.info` | `--alloy-color-status-info` | Informational banners |
| `color.status.success` | `--alloy-color-status-success` | Successful SAS crypto verification |
| `color.status.mention` | `--alloy-color-status-mention` | At-mention pill and unread badge highlight |
| `color.bubble.own.background` | `--alloy-color-bubble-own-background` | Outgoing message bubble background |
| `color.bubble.own.text` | `--alloy-color-bubble-own-text` | Outgoing message bubble text |
| `color.bubble.other.background`| `--alloy-color-bubble-other-background`| Incoming message bubble background |
| `color.bubble.other.text` | `--alloy-color-bubble-other-text` | Incoming message bubble text |
| `color.border.subtle` | `--alloy-color-border-subtle` | Pane dividers, thin separators |
| `color.border.prominent` | `--alloy-color-border-prominent` | Active input borders, card outlines |
| `color.user.1` .. `color.user.12` | `--alloy-color-user-1` .. `-12` | Sender identity colors (deterministic hash of user id; WCAG AA on surface) |

#### Typography Tokens (`--alloy-typography-*`)
| Token Path | CSS Custom Property | Example Default |
| :--- | :--- | :--- |
| `typography.font_family.sans` | `--alloy-typography-font-family-sans` | `Inter, -apple-system, sans-serif` |
| `typography.font_family.mono` | `--alloy-typography-font-family-mono` | `'JetBrains Mono', monospace` |
| `typography.font_family.display`| `--alloy-typography-font-family-display`| `Inter, -apple-system, sans-serif` |
| `typography.size.xs` | `--alloy-typography-size-xs` | `11px` |
| `typography.size.sm` | `--alloy-typography-size-sm` | `12px` |
| `typography.size.base` | `--alloy-typography-size-base` | `14px` |
| `typography.size.lg` | `--alloy-typography-size-lg` | `16px` |
| `typography.size.xl` | `--alloy-typography-size-xl` | `18px` |
| `typography.size.xxl` | `--alloy-typography-size-xxl` | `24px` |
| `typography.weight.regular` | `--alloy-typography-weight-regular` | `400` |
| `typography.weight.medium` | `--alloy-typography-weight-medium` | `500` |
| `typography.weight.semibold` | `--alloy-typography-weight-semibold` | `600` |
| `typography.weight.bold` | `--alloy-typography-weight-bold` | `700` |
| `typography.line_height.tight` | `--alloy-typography-line-height-tight`| `1.25` |
| `typography.line_height.normal`| `--alloy-typography-line-height-normal`| `1.5` |
| `typography.line_height.relaxed`|`--alloy-typography-line-height-relaxed`| `1.75` |

#### Spacing & Density Tokens (`--alloy-spacing-*`)
| Token Path | CSS Custom Property | Presets (Compact / Normal / Cozy) |
| :--- | :--- | :--- |
| `spacing.density` | `--alloy-spacing-density` | `"compact"`, `"normal"`, `"cozy"` |
| `spacing.message_padding` | `--alloy-spacing-message-padding` | `4px` / `8px` / `12px` |
| `spacing.avatar_size` | `--alloy-spacing-avatar-size` | `28px` / `36px` / `40px` |
| `spacing.scale.1` .. `scale.8` | `--alloy-spacing-scale-1` .. `-8` | `4px`, `8px`, `12px`, `16px`, `20px`, `24px`, `32px`, `40px` |

#### Radii Tokens (`--alloy-radii-*`)
| Token Path | CSS Custom Property | Value |
| :--- | :--- | :--- |
| `radii.none` | `--alloy-radii-none` | `0px` |
| `radii.sm` | `--alloy-radii-sm` | `4px` |
| `radii.md` | `--alloy-radii-md` | `8px` |
| `radii.lg` | `--alloy-radii-lg` | `12px` |
| `radii.full` | `--alloy-radii-full` | `9999px` |

#### Elevation & Shadows (`--alloy-elevation-*`)
| Token Path | CSS Custom Property | Description |
| :--- | :--- | :--- |
| `elevation.none` | `--alloy-elevation-none` | `none` |
| `elevation.sm` | `--alloy-elevation-sm` | Subtle card shadow |
| `elevation.md` | `--alloy-elevation-md` | Dropdown and popover shadow |
| `elevation.lg` | `--alloy-elevation-lg` | Modal dialog shadow |

#### Motion Tokens (`--alloy-motion-*`)
| Token Path | CSS Custom Property | Description |
| :--- | :--- | :--- |
| `motion.duration.fast` | `--alloy-motion-duration-fast` | `100ms` (micro-interactions, button active) |
| `motion.duration.normal` | `--alloy-motion-duration-normal` | `200ms` (drawer transitions, menu fades) |
| `motion.duration.slow` | `--alloy-motion-duration-slow` | `350ms` (large sheet animations) |
| `motion.easing.ease` | `--alloy-motion-easing-ease` | `ease` |
| `motion.easing.ease_in_out` | `--alloy-motion-easing-ease-in-out` | `cubic-bezier(0.4, 0, 0.2, 1)` |
| `motion.reduced_motion` | `--alloy-motion-reduced-motion` | `"true"` or `"false"` (respects OS setting) |

#### Layout Tokens (`--alloy-layout-*`)
| Token Path | CSS Custom Property | Allowed Values |
| :--- | :--- | :--- |
| `layout.message_style` | `--alloy-layout-message-style` | `bubble` (modern chat) or `gutter` (IRC/Slack) |
| `layout.avatar_shape` | `--alloy-layout-avatar-shape` | `circle`, `rounded`, `square` |

#### Chat Presentation Tokens (`--alloy-chat-*`)
| Token Path | CSS Custom Property | Description |
| :--- | :--- | :--- |
| `chat.background` | `--alloy-chat-background` | Background value or `none` |
| `chat.background.kind` | `--alloy-chat-background-kind` | `none`, `solid`, `gradient`, `pattern` |
| `chat.background.value` | `--alloy-chat-background-value` | CSS background definition |
| `chat.background.opacity` | `--alloy-chat-background-opacity` | Opacity value |

---

## 3. Theme File Format

Themes are declared in JSON/JSONC with `$schema` pointing to `schemas/theme.schema.json`:

```jsonc
{
  "$schema": "https://alloy.chat/schemas/v1/theme.json",
  "schema_version": 1,
  "id": "my-theme",
  "name": "My Custom Theme",
  "author": "Community",
  "version": "1.0.0",
  "mode": "dark",
  "extends": "alloy-dark", // Inherits all tokens from alloy-dark
  "tokens": {
    "color": {
      "accent": {
        "primary": "#0077b6"
      }
    },
    "layout": {
      "message_style": "gutter"
    }
  },
  "overrides": {
    "custom.app.brand": "oklch(0.65 0.22 260)"
  }
}
```

---

## 4. WCAG 2.1 Contrast Checker

`alloy-theme` includes a built-in WCAG 2.1 relative luminance and contrast engine. Every resolved theme undergoes automated contrast validation for key pairs:

- `color.text.primary` on `color.surface.canvas` ($\ge 4.5:1$ AA, $\ge 7.0:1$ AAA)
- `color.text.secondary` on `color.surface.canvas` ($\ge 4.5:1$ AA)
- `color.text.link` on `color.surface.canvas` ($\ge 3.0:1$ AA)
- `color.accent.primary` on `color.surface.canvas` ($\ge 3.0:1$ AA)
- `color.bubble.own.text` on `color.bubble.own.background` ($\ge 4.5:1$ AA)
- `color.bubble.other.text` on `color.bubble.other.background` ($\ge 4.5:1$ AA)
- `color.text.on_accent` on `color.accent.primary` ($\ge 4.5:1$ AA)

If a theme violates WCAG AA, `contrast_report.passes_aa` is false and `contrast_report.failures()` enumerates the exact violating pairs, observed ratios, and required thresholds.

---

## 5. Rust API Usage

```rust
use alloy_theme::{ThemeLoader, ThemeResolver};

// 1. Load theme from string or file (JSON or JSONC)
let theme = ThemeLoader::load_from_path("packages/themes/alloy-light.json")?;

// 2. Resolve against base hierarchy
let resolver = ThemeResolver::new();
let resolved = resolver.resolve(&theme)?;

// 3. Inspect contrast
if !resolved.contrast_report.passes_aa {
    for failure in resolved.contrast_report.failures() {
        eprintln!("Contrast violation: {} ratio {:.2}", failure.name, failure.ratio);
    }
}

// 4. Emit CSS variables
let css = resolved.to_css_vars();
println!("{}", css);
```
