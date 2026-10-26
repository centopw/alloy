---
id: adr-0010-secure-mobile-webview-origin
type: architecture-design
status: draft
title: "ADR 0010: Serve the webview over the https scheme so credential forms are a secure context"
parent: alloy-architecture
depends-on: [adr-0005-mobile-first-shell]
tags: [adr, security, android]
---

# 0010: Serve the webview over the https scheme so credential forms are a secure context

- Status: accepted
- Date: 2026-10-26
- Source: maintainer review of a production Android build: signing in warned that the form was not
  secure and that credentials could leak.

## Context

Tauri's custom protocol defaults to `http://tauri.localhost` on Android and Windows. The app's own
sign-in form (`Onboarding.svelte`, `autocomplete="username"` / `"current-password"`) is therefore
served from a non-secure origin. Android's autofill treats that origin as insecure and warns the user
that the form is not secure and autofill has been turned off — a warning no production messenger may
show, and one that also means password managers will not offer to fill the form.

The homeserver traffic itself is unaffected: it is Matrix over the engine, not the webview.

## Decision

Set `useHttpsScheme: true` on the Tauri window (`apps/alloy/src-tauri/tauri.conf.json`). The custom
protocol becomes `https://tauri.localhost` on Android and Windows, which is a secure context, so the
autofill warning disappears and password managers work. On macOS and Linux the setting is ignored
(those platforms already use a trusted `tauri://localhost` protocol).

## Alternatives considered

- **Disable autofill** (`generalAutofillEnabled: false`): suppresses the warning without changing the
  origin, but removes password-manager integration entirely — worse for users, who then type passwords
  by hand, and arguably less secure. Rejected.
- **A custom HTTPS certificate / local server**: unnecessary machinery for an origin Tauri already
  serves in-process. Rejected.

## Consequences

- Changing the scheme changes the webview storage origin, so `localStorage` resets once on upgrade for
  Android and Windows. Everything kept only there resets to its default: the theme choice
  (`alloy.theme.v1`), layout, font size, the notification toggles, Enter-to-send, autostart and the
  update toggles (the device-only keys of ADR 0007's prefs split), plus the read marks. The synced
  preferences (`chat.alloy.prefs`) reload from the engine on sign-in. Accepted by the maintainer.
- Android development builds still load the Vite dev server over `http://localhost:1420`, so the
  warning can still appear in `tauri android dev`. Only packaged builds are affected by this record.
- Any future decision to change the scheme again must treat the storage origin as a migration.
