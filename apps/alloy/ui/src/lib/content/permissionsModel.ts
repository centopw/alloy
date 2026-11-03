import type { PermissionId, PermissionState } from '../platform/permissions';

import { t } from '$lib/i18n';

/** One row of the Permissions screen, before its live state is known. */
export interface PermissionSpec {
  id: PermissionId;
  label: string;
  /** One line on what stops working without it. */
  why: string;
  /** Without a required permission the app cannot do its main job (tell you about new messages). */
  required: boolean;
  /** Only on Android. */
  androidOnly: boolean;
}

export const PERMISSION_SPECS: readonly PermissionSpec[] = [
  {
    id: 'notifications',
    label: t('sidebar-notifications'),
    why: t('permissions-notifications-why'),
    required: true,
    androidOnly: false,
  },
  {
    id: 'push-app',
    label: t('notifications-push-app'),
    why: t('permissions-push-app-why'),
    required: false,
    androidOnly: true,
  },
  {
    id: 'battery',
    label: t('permissions-battery-unrestricted'),
    why: t('permissions-battery-why'),
    required: false,
    androidOnly: true,
  },
  {
    id: 'microphone',
    label: t('permissions-microphone'),
    why: t('permissions-microphone-why'),
    required: false,
    androidOnly: false,
  },
  {
    id: 'location',
    label: t('location-card-location'),
    why: t('permissions-location-why'),
    required: false,
    androidOnly: false,
  },
];

/** The specs that exist on this platform, in screen order. */
export function specsFor(android: boolean): PermissionSpec[] {
  return PERMISSION_SPECS.filter((spec) => android || !spec.androidOnly);
}

/** The pill text for a state. */
export function stateLabel(state: PermissionState): string {
  switch (state) {
    case 'granted':
      return t('permissions-state-allowed');
    case 'denied':
      return t('permissions-state-not-allowed');
    case 'prompt':
    case 'unknown':
      return t('permissions-state-not-asked-yet');
    case 'unavailable':
      return t('permissions-state-not-available');
  }
}

/** What the row's button does, or null when there is nothing for the user to do. */
export type PermissionAction = 'allow' | 'open-settings' | null;

export function actionFor(state: PermissionState, canOpenSettings: boolean): PermissionAction {
  if (state === 'granted' || state === 'unavailable') return null;
  if (state === 'denied') return canOpenSettings ? 'open-settings' : null;
  return 'allow';
}

export interface PermissionSummary {
  /** Required permissions the user still has to allow. */
  missingRequired: number;
  requiredTotal: number;
  /** "2 of 3 required permissions allowed", or a line saying all is set. */
  text: string;
}

export function summarize(rows: readonly { required: boolean; state: PermissionState }[]): PermissionSummary {
  const required = rows.filter((r) => r.required && r.state !== 'unavailable');
  const allowed = required.filter((r) => r.state === 'granted').length;
  const missingRequired = required.length - allowed;
  const text =
    required.length === 0
      ? 'Nothing here is required on this device.'
      : missingRequired === 0
        ? 'Everything Alloy needs is allowed.'
        : `${allowed} of ${required.length} required ${required.length === 1 ? 'permission' : 'permissions'} allowed`;
  return { missingRequired, requiredTotal: required.length, text };
}

/**
 * Whether the first-run prompt shows: only while a required permission is missing and the user has not
 * dismissed it. A permission the platform cannot grant never counts as missing, so nothing is blocked for nothing.
 */
export function shouldPrompt(
  rows: readonly { required: boolean; state: PermissionState }[],
  dismissed: boolean
): boolean {
  return !dismissed && summarize(rows).missingRequired > 0;
}
