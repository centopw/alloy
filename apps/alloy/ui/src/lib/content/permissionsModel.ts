import type { PermissionId, PermissionState } from '../platform/permissions';

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
    label: 'Notifications',
    why: 'Shows new messages when Alloy is not in front of you.',
    required: true,
    androidOnly: false,
  },
  {
    id: 'push-app',
    label: 'Push app',
    why: 'A UnifiedPush app such as ntfy delivers messages while Alloy is closed, without Google services.',
    required: false,
    androidOnly: true,
  },
  {
    id: 'battery',
    label: 'Unrestricted battery',
    why: 'Stops Android from delaying messages while the phone is idle.',
    required: false,
    androidOnly: true,
  },
  {
    id: 'microphone',
    label: 'Microphone',
    why: 'Records voice messages.',
    required: false,
    androidOnly: false,
  },
  {
    id: 'location',
    label: 'Location',
    why: 'Shares where you are when you send your location.',
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
      return 'Allowed';
    case 'denied':
      return 'Not allowed';
    case 'prompt':
    case 'unknown':
      return 'Not asked yet';
    case 'unavailable':
      return 'Not available';
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
