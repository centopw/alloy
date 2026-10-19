import { viewport } from '../stores/viewport.svelte';
import { isTauriShell } from './notify';
import { isAutostartSupported } from './autostart';

export { isTauriShell, isAutostartSupported };

/**
 * Returns true if the environment is Android based on userAgent or navigator platform.
 * Supports explicit string overrides for deterministic unit testing.
 */
export function isAndroid(
  userAgent: string = typeof navigator !== 'undefined' ? navigator.userAgent : '',
  platform: string = typeof navigator !== 'undefined'
    ? ((navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform || navigator.platform || '')
    : ''
): boolean {
  if (platform && /android/i.test(platform)) return true;
  return /android/i.test(userAgent);
}

export interface PlatformEnv {
  phone: boolean;
  android: boolean;
  tauri: boolean;
}

export interface PlatformCapabilities {
  /** Physical keyboard shortcut navigation and shortcuts dialog. */
  keyboardShortcuts: boolean;
  /** Resizable column divider panels. */
  resizablePanels: boolean;
  /** Persistent multi-column sidebar next to chat. */
  sidebarLayout: boolean;
  /** Send message with Enter (phones always use newline on Enter). */
  enterToSend: boolean;
  /** Launch application at system login. */
  autostart: boolean;
  /** OS desktop notifications terminology vs generic notifications. */
  desktopNotifications: boolean;
  /** Option to notify only when the desktop window is unfocused. */
  windowFocusOptions: boolean;
  /** Desktop system tray integration. */
  systemTray: boolean;
  /** In-place update install and background download (desktop app only; Android opens the release page). */
  autoDownloadUpdates: boolean;
}

/**
 * Pure function deriving feature capabilities from the platform environment flags.
 */
export function capabilities(env: PlatformEnv): PlatformCapabilities {
  const isDesktop = !env.phone && !env.android;
  return {
    keyboardShortcuts: isDesktop,
    resizablePanels: !env.phone,
    sidebarLayout: !env.phone,
    enterToSend: isDesktop,
    autostart: env.tauri && isDesktop,
    desktopNotifications: isDesktop,
    windowFocusOptions: isDesktop,
    systemTray: env.tauri && isDesktop,
    autoDownloadUpdates: env.tauri && isDesktop,
  };
}

/**
 * Resolves the live platform environment.
 */
export function currentEnv(phone: boolean = viewport.isPhone): PlatformEnv {
  return {
    phone,
    android: isAndroid(),
    tauri: isTauriShell(),
  };
}

export type SettingsSectionId =
  | 'account'
  | 'devices'
  | 'appearance'
  | 'notifications'
  | 'preferences'
  | 'keyboard'
  | 'sidebar'
  | 'tags'
  | 'security-privacy'
  | 'encryption'
  | 'help-about';

export interface RawSettingsItem<TIcon = unknown> {
  id: SettingsSectionId;
  label: string;
  phoneLabel?: string;
  icon?: TIcon;
  phoneIcon?: TIcon;
  visible?: (caps: PlatformCapabilities) => boolean;
}

export interface RawSettingsGroup<TIcon = unknown> {
  label: string;
  items: RawSettingsItem<TIcon>[];
}

export interface VisibleSettingsItem<TIcon = unknown> {
  id: SettingsSectionId;
  label: string;
  icon?: TIcon;
}

export interface VisibleSettingsGroup<TIcon = unknown> {
  label: string;
  items: VisibleSettingsItem<TIcon>[];
}

export const SETTINGS_GROUPS_DEF: RawSettingsGroup[] = [
  {
    label: 'User settings',
    items: [
      { id: 'account', label: 'Account' },
      { id: 'devices', label: 'Devices' },
      { id: 'security-privacy', label: 'Security & Privacy' },
      { id: 'encryption', label: 'Encryption' },
    ],
  },
  {
    label: 'App settings',
    items: [
      { id: 'appearance', label: 'Appearance' },
      { id: 'notifications', label: 'Notifications' },
      { id: 'preferences', label: 'Preferences' },
      { id: 'keyboard', label: 'Keyboard', visible: (caps) => caps.keyboardShortcuts },
      { id: 'sidebar', label: 'Sidebar', phoneLabel: 'Chat list' },
      { id: 'tags', label: 'Tags' },
    ],
  },
  {
    label: 'Info',
    items: [{ id: 'help-about', label: 'Help & About' }],
  },
];

/**
 * Filters settings groups according to the given platform capabilities.
 * Adapts section labels (e.g. 'Sidebar' -> 'Chat list' when phones have no sidebar),
 * switches to phoneIcon if specified, and omits groups whose items are all hidden.
 */
export function filterSettingsGroups<TIcon>(
  groups: RawSettingsGroup<TIcon>[],
  caps: PlatformCapabilities
): VisibleSettingsGroup<TIcon>[] {
  return groups
    .map((group) => {
      const items: VisibleSettingsItem<TIcon>[] = [];
      for (const item of group.items) {
        if (item.visible && !item.visible(caps)) {
          continue;
        }
        const label = !caps.sidebarLayout && item.id === 'sidebar'
          ? (item.phoneLabel ?? 'Chat list')
          : item.label;
        const icon = !caps.sidebarLayout && item.phoneIcon ? item.phoneIcon : item.icon;
        items.push({
          id: item.id,
          label,
          icon,
        });
      }
      return {
        label: group.label,
        items,
      };
    })
    .filter((group) => group.items.length > 0);
}
