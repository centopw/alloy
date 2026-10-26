---
name: Modern Technical Workspace
colors:
  surface: '#111318'
  surface-dim: '#111318'
  surface-bright: '#37393f'
  surface-container-lowest: '#0c0e13'
  surface-container-low: '#1a1b21'
  surface-container: '#1e1f25'
  surface-container-high: '#282a2f'
  surface-container-highest: '#33353a'
  on-surface: '#e2e2e9'
  on-surface-variant: '#c2c6d6'
  inverse-surface: '#e2e2e9'
  inverse-on-surface: '#2e3036'
  outline: '#8c909f'
  outline-variant: '#424754'
  surface-tint: '#adc6ff'
  primary: '#adc6ff'
  on-primary: '#002e6a'
  primary-container: '#4d8eff'
  on-primary-container: '#00285d'
  inverse-primary: '#005ac2'
  secondary: '#4edea3'
  on-secondary: '#003824'
  secondary-container: '#00a572'
  on-secondary-container: '#00311f'
  tertiary: '#ffb95f'
  on-tertiary: '#472a00'
  tertiary-container: '#ca8100'
  on-tertiary-container: '#3e2400'
  error: '#ffb4ab'
  on-error: '#690005'
  error-container: '#93000a'
  on-error-container: '#ffdad6'
  primary-fixed: '#d8e2ff'
  primary-fixed-dim: '#adc6ff'
  on-primary-fixed: '#001a42'
  on-primary-fixed-variant: '#004395'
  secondary-fixed: '#6ffbbe'
  secondary-fixed-dim: '#4edea3'
  on-secondary-fixed: '#002113'
  on-secondary-fixed-variant: '#005236'
  tertiary-fixed: '#ffddb8'
  tertiary-fixed-dim: '#ffb95f'
  on-tertiary-fixed: '#2a1700'
  on-tertiary-fixed-variant: '#653e00'
  background: '#111318'
  on-background: '#e2e2e9'
  surface-variant: '#33353a'
typography:
  headline-xl:
    fontFamily: Inter
    fontSize: 24px
    fontWeight: '600'
    lineHeight: 32px
    letterSpacing: -0.02em
  headline-lg:
    fontFamily: Inter
    fontSize: 18px
    fontWeight: '600'
    lineHeight: 24px
    letterSpacing: -0.015em
  headline-sm:
    fontFamily: Inter
    fontSize: 15px
    fontWeight: '600'
    lineHeight: 20px
    letterSpacing: -0.01em
  body-lg:
    fontFamily: Inter
    fontSize: 15px
    fontWeight: '400'
    lineHeight: 22px
    letterSpacing: -0.005em
  body-md:
    fontFamily: Inter
    fontSize: 14px
    fontWeight: '400'
    lineHeight: 20px
    letterSpacing: 0em
  body-sm:
    fontFamily: Inter
    fontSize: 13px
    fontWeight: '400'
    lineHeight: 18px
    letterSpacing: 0em
  label-md:
    fontFamily: Inter
    fontSize: 12px
    fontWeight: '500'
    lineHeight: 16px
    letterSpacing: 0.01em
  label-sm:
    fontFamily: Inter
    fontSize: 11px
    fontWeight: '600'
    lineHeight: 14px
    letterSpacing: 0.03em
  code-sm:
    fontFamily: JetBrains Mono
    fontSize: 12px
    fontWeight: '400'
    lineHeight: 16px
    letterSpacing: 0em
rounded:
  sm: 0.125rem
  DEFAULT: 0.25rem
  md: 0.375rem
  lg: 0.5rem
  xl: 0.75rem
  full: 9999px
spacing:
  gutter: 0px
  margin: 0px
  space-2xs: 0.125rem
  space-xs: 0.25rem
  space-sm: 0.5rem
  space-md: 0.75rem
  space-lg: 1rem
  space-xl: 1.5rem
  space-2xl: 2rem
---

## Brand & Style

This design system targets high-velocity software teams, distributed technical communities, and power communicators who demand absolute clarity, speed, and spatial efficiency. Drawing from the disciplined utility of Linear and the conversational fluidity of Telegram Desktop and Slack, the visual character is grounded, architectural, and utilitarian.

The aesthetic follows an understated, high-density dark mode approach:
- **Tone:** Professional, precise, whisper-quiet, and engineering-centric.
- **Visual Discipline:** Zero decorative noise, zero diffuse ambient glows, and zero non-functional gradients.
- **Hierarchy:** Structural integrity relies on strictly nested structural panes, hairline boundary dividers (`1px`), subtle tint-shifted surface layers, and purposeful high-contrast accents.
- **Interaction Feel:** Snappy, instantaneous transitions (under 120ms) with tactile, distinct hover and active states designed for keyboard-driven workflows.

## Colors

The palette is engineered exclusively for prolonged dark-room productivity, offering calibrated contrast ratios that exceed WCAG 2.1 AA requirements while mitigating eye fatigue.

### Surface Architecture & Foundations
- **Base Canvas (`#0e1015`):** The primary view layer for the central chat timeline, message stage, and root background.
- **Secondary Surface (`#151821`):** Applied to lateral structural containers, including the global server/workspace rail, room channel navigators, and the right inspector panel.
- **Elevated Floating Surface (`#1c202d`):** Dedicated to modals, contextual popovers, dropdown flyouts, and floating message action bars.
- **Active & Selected Surface (`#252a3a`):** Interactive selections, highlighted channel rows, and focused items.
- **Subtle Hover Surface (`#1a1e2b`):** Immediate mouseover feedback for interactive rows, toolbar buttons, and menu elements.

### Structural Strokes & Dividers
- **Hairline Border (`#232734`):** The universal separator used for all panel partitions, dividers, table headers, and structural fences.
- **Focus / Active Border (`#3b82f6`):** Applied during keyboard focus states, inline editor focuses, and selected state outlines.

### Functional Accents
- **Brand Primary Accent (`#3b82f6`):** Vibrant cobalt blue reserved strictly for actionable affordances: active indicators, primary submit buttons, unread badges, mention highlights, and pinned focus states.
- **Success (`#10b981`):** Active user presence indicators, operational connection states, and successful execution toasts.
- **Warning / Alert (`#f59e0b`):** Warning banners, pending verification states, and rate-limit notices.
- **Danger / Destructive (`#ef4444`):** Revoke actions, disconnect warnings, error states, and destructive channel operations.

### Typography Hierarchy
- **Text Primary (`#f1f5f9`):** Message body strings, input text, dialog titles, and primary headers.
- **Text Secondary (`#94a3b8`):** Channel titles, section metadata, active timestamps, and secondary button text.
- **Text Muted (`#64748b`):** Inactive timestamps, system events, keyboard shortcut tooltips, and ghosted placeholders.

## Typography

The type scale is built around `Inter`, prioritizing neutral clarity, tall x-height, and legible glyph distinctions at dense interface scales. Code elements leverage `JetBrains Mono` for cryptographic identifiers, markdown blocks, and log traces.

### Functional Applications
- **`headline-xl` (24px/600):** Top-level modal titles, empty workspace display states, full-screen room settings.
- **`headline-lg` (18px/600):** Room timeline headers, inspector drawer headers, major popover headings.
- **`headline-sm` (15px/600):** Author names in grouped messages, card titles, section category titles.
- **`body-lg` (15px/400):** Primary message body text in standard timeline layouts, optimized for sustained reading.
- **`body-md` (14px/400):** Compact chat mode, input composer text, inspector key-value descriptions.
- **`body-sm` (13px/400):** Channel item lists, user bio previews, thread reply counts, rich embed sub-descriptions.
- **`label-md` (12px/500):** Timestamps, filter tags, reactions counts, composer mode switches.
- **`label-sm` (11px/600, uppercase):** Section dividers (e.g., "TEXT CHANNELS", "DIRECT MESSAGES"), keyboard key badge indicators, unread count pills.
- **`code-sm` (12px/400):** Inline markdown backticks, room IDs, event hashes, and server matrix addresses.

## Layout & Spacing

This layout uses a continuous multi-pane workspace without external gutters or outer canvas margins. The application viewport relies on edge-to-edge docked columns separated by precise `1px` borders.

### Column Architecture (Desktop Layout)
1. **Workspace Icon Rail (Far Left):** Fixed width of `68px`. Holds primary server hubs, personal status, user avatar, and global settings toggles.
2. **Channel & Room Sidebar:** Fixed width of `240px` (resizable between `200px` and `320px`). Hosts room lists, filter tags, active voice channels, and direct chats.
3. **Primary Chat Timeline (Center):** Fluid stretch container (minimum `480px`). Contains room header, scrollable virtualization message list, and sticky bottom composer.
4. **Collapsible Inspector / Thread Drawer (Far Right):** Collapsible panel (`320px` to `420px`). Hosts room details, pinned items, media galleries, or contextual reply threads.

### Adaptive Behavior
- **Desktop (>= 1280px):** 4-pane layout fully expanded (Rail + Channels + Timeline + Inspector).
- **Tablet / Narrow Window (768px – 1279px):** Inspector defaults to an off-canvas overlay panel; channels sidebar can collapse into an icon drawer (`60px`).
- **Mobile (< 768px):** Single-column stacked state. Rail switches to bottom navigation; channels and timeline operate on a drill-down navigation stack.

### Internal Spacing Cadence
- **`space-2xs` (2px):** Micro gaps between nested status dots and labels.
- **`space-xs` (4px):** Gaps between message reaction pills, tag padding, list item vertical spacing.
- **`space-sm` (8px):** Spacing between grouped avatar and message text; padding inside compact buttons and inputs.
- **`space-md` (12px):** Standard padding for sidebar room rows, popover list items, and reaction bars.
- **`space-lg` (16px):** Timeline outer horizontal gutters, message composer interior padding, panel header padding.
- **`space-xl` (24px):** Inspector container padding, modal container interior margin.

## Elevation & Depth

Visual hierarchy does not use soft colored glows, warm skeuomorphism, or high-contrast drop shadows. Depth is achieved via **tonal layering**, **hairline boundaries**, and **deliberate backdrop filtration**.

### Elevation Tiers
- **Tier 0 (Root Timeline / `#0e1015`):** Lowest layer. Non-elevated, clean matte canvas.
- **Tier 1 (Peripheral Structural Rails / `#151821`):** Bordered with a right or left `1px solid #232734` boundary. Creates architectural containment without cast shadows.
- **Tier 2 (Sticky Headers & Inline Elements / `#0e1015` with 85% opacity):** Timestamps dividers, pinned timeline summaries, and top navbars utilize `backdrop-filter: blur(12px)` and a bottom border of `1px solid #232734`.
- **Tier 3 (Floating Overlays & Context Menus / `#1c202d`):** Dropdowns, emoji pickers, floating action bars, and autocomplete drawers feature a `1px solid #2e3446` border and a crisp shadow: `0 8px 24px -4px rgba(0, 0, 0, 0.6)`.
- **Tier 4 (System Modals):** Centered dialogs sitting on `#1c202d` with a `1px solid #333a4d` frame, surrounded by a scrim overlay of `#05070a` at `70% opacity` with `backdrop-filter: blur(8px)`.

## Shapes

The design system adopts a crisp, modern geometric shape logic (soft level `1`). Elements balance structural sharpness with slight corner rounding to prevent visual fatigue while preserving dense information alignment.

### Corner Radii Guidelines
- **Zero Radius (`0px`):** All major docked panel interfaces (rail, sidebar, inspector boundaries), split-view dividers, and window frame edges.
- **Soft Component Radius (`4px` - default):** Reaction pills, filter chips, keyboard shortcut badges (`kbd`), context menu options, and timeline embed attachments.
- **Medium Radius (`6px`):** Form inputs, message composer wrapper, modal dialog shells, toast alerts, and buttons.
- **Circular (`9999px`):** User presence status dots, count notification pills, and avatar roundels (except workspace icons, which use `8px` smooth squircle transitions).

## Components

### Workspace Icon Rail (Far-Left)
- **Dimensions & Layout:** Exactly `68px` wide, vertically stacked, centered items with `12px` vertical gap.
- **Workspace Buttons:** `44x44px` squircle (`8px` radius). Background `#1c202d` with a `1px solid #232734` border. Hover state morphs background to `#252a3a` and brightens border to `#3b82f6`.
- **Active State Indicator:** An absolute left-aligned bar (`3px` wide, `24px` high, `#3b82f6`, rounded right edge) sliding vertically to match the active workspace.

### Channels & Room List
- **Section Headers:** `label-sm` in `#64748b`, uppercase, with `8px` vertical padding, expandable toggle arrow icon.
- **Room Rows:** Height `32px`, horizontal padding `8px`, border radius `4px`. Text in `#94a3b8` (`body-sm`).
- **Row Interaction:** Hover applies background `#1a1e2b` and text `#f1f5f9`. Active state sets background `#252a3a`, text `#f1f5f9`, font-weight `500`.
- **Unread & Mentions:** Bold title text `#f1f5f9`, small unread dot (`6px`, `#3b82f6`) or badge pill with mention count (`#3b82f6` background, `#ffffff` text, `label-sm`).

### Message Timeline & Message Groups
- **Grouped Stream Layout:** Avatar displayed only on initial message of a continuous cluster; subsequent messages inside a 5-minute window indent to match the `40px` avatar margin.
- **Hover State:** Entire message group highlights with `#151821` at full width; reveals a floating micro-action toolbar (Add Reaction, Reply in Thread, More) anchored top-right with background `#1c202d` and `1px solid #232734`.
- **Mentions & Targets:** Messages mentioning the current user render with an inset left border `2px solid #3b82f6` and background tint `rgba(59, 130, 246, 0.08)`.

### Message Composer
- **Container:** Grounded at timeline base with horizontal margin `16px`, bottom margin `16px`. Wrapped in a `1px solid #232734` frame with background `#151821` and `rounded-md` (`6px`).
- **Focus State:** Border shifts to `1px solid #3b82f6` with zero outer glow.
- **Structure:** Multi-line auto-expanding editor (`body-md`), bottom accessory bar containing formatting tools (bold, code, attachments, emoji trigger) and a dedicated send button (`28x28px`, background `#3b82f6`, icon white).

### Rich Embed Cards & Code Blocks
- **Card Shell:** Background `#151821`, border `1px solid #232734`, border-radius `4px`, left border indicator `3px solid #3b82f6` (or specific service color). Max width `540px`.
- **Code Blocks:** Full-width or inline rounded rects with background `#0a0c10` and hairline border `#1e2230`. Top strip displaying language tag in `label-sm` muted text with copy button.

### Filter Tags & Reaction Chips
- **Reactions:** Background `#151821`, border `1px solid #232734`, height `24px`, padding `2px 6px`, radius `4px`. Contains emoji + count (`label-md`). Hover: border `#3b82f6`. Active (user reacted): background `rgba(59, 130, 246, 0.15)`, border `#3b82f6`, text `#3b82f6`.

### Buttons
- **Primary:** Background `#3b82f6`, text `#ffffff`, height `32px`, padding `0 12px`, radius `4px`, font weight `500`. Hover: `#2563eb`. Active: `#1d4ed8`.
- **Secondary:** Background `#1c202d`, text `#f1f5f9`, border `1px solid #232734`. Hover: background `#252a3a`, border `#333a4d`.
- **Ghost:** Background transparent, text `#94a3b8`. Hover: background `#1a1e2b`, text `#f1f5f9`.

### Modal Dialogs
- **Backdrop:** `#05070a` with 70% opacity, `backdrop-filter: blur(8px)`.
- **Card Shell:** Width `480px` to `560px`, background `#1c202d`, border `1px solid #2e3446`, radius `6px`, padding `24px`. Title in `headline-lg`. Explicit hairline divider above the footer actions.
