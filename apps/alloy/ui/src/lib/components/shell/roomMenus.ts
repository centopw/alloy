import type { RoomSummary } from '$lib/generated';
import { Copy } from '$lib/icons';
import { copyText } from '$lib/platform/clipboard';
import { contextMenu, type MenuItem } from '$lib/stores/contextmenu.svelte';
import { isRoomMuted, roomsStore } from '$lib/stores/rooms.svelte';
import { muteRoom, unmuteRoom } from '$lib/stores/muteTimer.svelte';
import { MUTE_DURATIONS } from '$lib/content/mute';

import { t } from '$lib/i18n';

const roomOf = (roomId: string) => roomsStore.rooms.find((r) => r.id === roomId) ?? null;

/** Notification menu for a chat: unmute, or mute for one of the offered durations. */
export function notifyItems(roomId: string): MenuItem[] {
  const room = roomOf(roomId);
  const muted = room ? isRoomMuted(room) : false;
  return [
    {
      id: 'notify-unmuted',
      label: t('chat-notifications-default'),
      selected: !muted,
      onselect: () => void unmuteRoom(roomId),
    },
    ...MUTE_DURATIONS.map(
      ({ label, ms }): MenuItem => ({
        id: `mute-${ms ?? 'forever'}`,
        label,
        onselect: () => void muteRoom(roomId, ms),
      }),
    ),
  ];
}

/** Copy actions for a chat: its local id, which the engine and the store share. */
export function copyItems(room: RoomSummary): MenuItem[] {
  return [
    {
      id: 'copy-room-id',
      label: t('room-menu-copy-room-id'),
      icon: Copy,
      onselect: () => void copyText(room.id).catch(() => undefined),
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
