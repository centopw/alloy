import type { NotificationLevel, RoomSummary } from '$lib/generated';
import { Copy, ImagePlus, Trash2 } from '$lib/icons';
import { copyText } from '$lib/platform/clipboard';
import { pickFile } from '$lib/platform/files';
import { contextMenu, type MenuItem } from '$lib/stores/contextmenu.svelte';
import { roomInfo } from '$lib/stores/roomInfo.svelte';
import { roomNotify } from '$lib/stores/roomNotify.svelte';
import { muteRoom, setRoomLevel } from '$lib/stores/muteTimer.svelte';
import { MUTE_DURATIONS } from '$lib/content/mute';
import { ui } from '$lib/stores/ui.svelte';

/** Notification choices in display order. `null` restores the account default. */
const NOTIFY_LEVELS: readonly { target: NotificationLevel | null; label: string }[] = [
  { target: null, label: 'Use default' },
  { target: 'All', label: 'All messages' },
  { target: 'Mentions', label: 'Mentions and keywords only' },
  { target: 'Mute', label: 'Mute' },
];

function isCurrentLevel(roomId: string, target: NotificationLevel | null): boolean {
  const isDefault = roomNotify.isDefault(roomId);
  return target === null
    ? isDefault === true
    : isDefault === false && roomNotify.levelOf(roomId) === target;
}

/** Notification level menu for a room; the current choice is `selected`. */
export function notifyItems(roomId: string): MenuItem[] {
  return NOTIFY_LEVELS.map(({ target, label }): MenuItem => {
    const base = { id: `notify-${target ?? 'default'}`, label, selected: isCurrentLevel(roomId, target), disabled: roomNotify.saving(roomId) };
    if (target === 'Mute') {
      return {
        ...base,
        submenu: {
          items: MUTE_DURATIONS.map(({ label: durationLabel, ms }): MenuItem => ({
            id: `mute-${ms ?? 'forever'}`,
            label: durationLabel,
            onselect: () => void muteRoom(roomId, ms),
          })),
        },
      };
    }
    return { ...base, onselect: () => void setRoomLevel(roomId, target) };
  });
}

/** Accessible label for the bell button: the room's current notification choice. */
export function notifyLabel(roomId: string): string {
  const current = NOTIFY_LEVELS.find(({ target }) => isCurrentLevel(roomId, target));
  return current ? `Notifications: ${current.label}` : 'Notifications';
}

/** Copy actions for a room. A direct room leads with the other person's Matrix ID. */
export function copyItems(room: RoomSummary): MenuItem[] {
  const targets = [
    ...(room.direct_user_id ? [{ id: 'copy-user-id', label: 'Copy user ID', value: room.direct_user_id }] : []),
    { id: 'copy-room-id', label: 'Copy room ID', value: room.id },
  ];
  return targets.map(({ id, label, value }) => ({
    id,
    label,
    icon: Copy,
    onselect: () => void copyText(value).catch(() => undefined),
  }));
}

async function replacePicture(roomId: string): Promise<void> {
  try {
    const path = await pickFile();
    if (!path) return;
    await roomInfo.setAvatar(roomId, path);
  } catch {
    /* the picker refused; the failure path below still reports write errors */
  }
  // Failures are kept on the room's info state; the details panel shows them.
  if (roomInfo.error(roomId)) ui.openRoomInfo('info');
}

/** Room picture actions, shown when the user presses the room's picture. Disabled without permission. */
export function roomPictureItems(roomId: string): MenuItem[] {
  const details = roomInfo.details(roomId);
  const canSet = details?.can_set_avatar === true;
  const busy = roomInfo.saving(roomId);
  return [
    {
      id: 'picture-replace',
      label: 'Replace picture...',
      icon: ImagePlus,
      disabled: !canSet || busy,
      onselect: () => void replacePicture(roomId),
    },
    {
      id: 'picture-remove',
      label: 'Remove picture',
      icon: Trash2,
      danger: true,
      disabled: !canSet || busy || !details?.avatar_url,
      onselect: () => void roomInfo.setAvatar(roomId, null),
    },
  ];
}

/** Opens `items` as a menu under the element the user pressed (`from`, usually `e.currentTarget`). */
export function openMenuFrom(from: EventTarget | null, items: MenuItem[], ariaLabel: string): void {
  if (!(from instanceof HTMLElement)) return;
  const box = from.getBoundingClientRect();
  contextMenu.open(
    items,
    { x: box.left, y: box.bottom },
    { anchor: { x: box.left, y: box.top, width: box.width, height: box.height }, ariaLabel },
  );
}
