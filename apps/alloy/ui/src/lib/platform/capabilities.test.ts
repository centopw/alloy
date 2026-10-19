import { describe, expect, it } from 'vitest';
import {
  capabilities,
  filterSettingsGroups,
  isAndroid,
  SETTINGS_GROUPS_DEF,
  type RawSettingsGroup,
} from './capabilities';

describe('isAndroid', () => {
  it('detects Android from userAgent', () => {
    const androidUa =
      'Mozilla/5.0 (Linux; U; Android 14; en-us; Pixel 8 Pro Build/UQ1A.231205.015) AppleWebKit/537.36';
    expect(isAndroid(androidUa, '')).toBe(true);
  });

  it('detects Android from client platform', () => {
    expect(isAndroid('', 'Android')).toBe(true);
    expect(isAndroid('', 'Linux armv8l (Android)')).toBe(true);
  });

  it('returns false for non-Android platforms', () => {
    const macUa =
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36';
    const winUa =
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36';
    const iosUa =
      'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1';

    expect(isAndroid(macUa, 'MacIntel')).toBe(false);
    expect(isAndroid(winUa, 'Win32')).toBe(false);
    expect(isAndroid(iosUa, 'iPhone')).toBe(false);
    expect(isAndroid('', '')).toBe(false);
  });
});

describe('capabilities', () => {
  it('returns full desktop capability set on desktop Tauri shell', () => {
    const caps = capabilities({ phone: false, android: false, tauri: true });
    expect(caps).toEqual({
      keyboardShortcuts: true,
      resizablePanels: true,
      sidebarLayout: true,
      enterToSend: true,
      autostart: true,
      desktopNotifications: true,
      windowFocusOptions: true,
      systemTray: true,
      autoDownloadUpdates: true,
    });
  });

  it('disables autostart and system tray in desktop browser build', () => {
    const caps = capabilities({ phone: false, android: false, tauri: false });
    expect(caps).toEqual({
      keyboardShortcuts: true,
      resizablePanels: true,
      sidebarLayout: true,
      enterToSend: true,
      autostart: false,
      desktopNotifications: true,
      windowFocusOptions: true,
      systemTray: false,
      autoDownloadUpdates: false,
    });
  });

  it('disables desktop-only features on phones', () => {
    const caps = capabilities({ phone: true, android: true, tauri: true });
    expect(caps).toEqual({
      keyboardShortcuts: false,
      resizablePanels: false,
      sidebarLayout: false,
      enterToSend: false,
      autostart: false,
      desktopNotifications: false,
      windowFocusOptions: false,
      systemTray: false,
      autoDownloadUpdates: false,
    });
  });

  it('disables desktop-only features on phone browser', () => {
    const caps = capabilities({ phone: true, android: false, tauri: false });
    expect(caps).toEqual({
      keyboardShortcuts: false,
      resizablePanels: false,
      sidebarLayout: false,
      enterToSend: false,
      autostart: false,
      desktopNotifications: false,
      windowFocusOptions: false,
      systemTray: false,
      autoDownloadUpdates: false,
    });
  });

  it('supports tablet panel layout while suppressing desktop-specific features on Android tablets', () => {
    const caps = capabilities({ phone: false, android: true, tauri: true });
    expect(caps).toEqual({
      keyboardShortcuts: false,
      resizablePanels: true,
      sidebarLayout: true,
      enterToSend: false,
      autostart: false,
      desktopNotifications: false,
      windowFocusOptions: false,
      systemTray: false,
      autoDownloadUpdates: false,
    });
  });
});

describe('filterSettingsGroups', () => {
  it('includes keyboard shortcuts and uses Sidebar title on desktop', () => {
    const desktopCaps = capabilities({ phone: false, android: false, tauri: true });
    const groups = filterSettingsGroups(SETTINGS_GROUPS_DEF, desktopCaps);

    const appSettings = groups.find((g) => g.label === 'App settings');
    expect(appSettings).toBeDefined();

    const itemIds = appSettings?.items.map((i) => i.id);
    expect(itemIds).toContain('keyboard');
    expect(itemIds).toContain('sidebar');

    const sidebarItem = appSettings?.items.find((i) => i.id === 'sidebar');
    expect(sidebarItem?.label).toBe('Sidebar');
  });

  it('excludes keyboard shortcuts and renames Sidebar to Chat list on phone', () => {
    const phoneCaps = capabilities({ phone: true, android: false, tauri: false });
    const groups = filterSettingsGroups(SETTINGS_GROUPS_DEF, phoneCaps);

    const appSettings = groups.find((g) => g.label === 'App settings');
    expect(appSettings).toBeDefined();

    const itemIds = appSettings?.items.map((i) => i.id);
    expect(itemIds).not.toContain('keyboard');
    expect(itemIds).toContain('sidebar');

    const sidebarItem = appSettings?.items.find((i) => i.id === 'sidebar');
    expect(sidebarItem?.label).toBe('Chat list');
  });

  it('removes groups where all items are hidden', () => {
    const caps = capabilities({ phone: true, android: false, tauri: false });
    const customGroups: RawSettingsGroup[] = [
      {
        label: 'Only Keyboard',
        items: [{ id: 'keyboard', label: 'Keyboard', visible: (c) => c.keyboardShortcuts }],
      },
      {
        label: 'Info',
        items: [{ id: 'help-about', label: 'Help & About' }],
      },
    ];

    const filtered = filterSettingsGroups(customGroups, caps);
    expect(filtered).toHaveLength(1);
    expect(filtered[0].label).toBe('Info');
  });

  it('swaps icon to phoneIcon on phone viewport', () => {
    const desktopIcon = 'desktop-icon';
    const mobileIcon = 'mobile-icon';
    const groupsWithIcons: RawSettingsGroup<string>[] = [
      {
        label: 'App',
        items: [
          {
            id: 'sidebar',
            label: 'Sidebar',
            phoneLabel: 'Chat list',
            icon: desktopIcon,
            phoneIcon: mobileIcon,
          },
        ],
      },
    ];

    const desktopGroups = filterSettingsGroups(
      groupsWithIcons,
      capabilities({ phone: false, android: false, tauri: true })
    );
    expect(desktopGroups[0].items[0].icon).toBe(desktopIcon);
    expect(desktopGroups[0].items[0].label).toBe('Sidebar');

    const phoneGroups = filterSettingsGroups(
      groupsWithIcons,
      capabilities({ phone: true, android: false, tauri: false })
    );
    expect(phoneGroups[0].items[0].icon).toBe(mobileIcon);
    expect(phoneGroups[0].items[0].label).toBe('Chat list');
  });
});
