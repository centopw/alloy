import { client } from '../ipc/index.js';
import { errorMessage, expect } from './errors.js';
import { roomsStore } from './rooms.svelte.js';
import { timelineStore } from './timeline.svelte.js';
import { tags } from './tags.svelte.js';
import { prefs } from './prefs.svelte.js';

/**
 * The manual Refresh: reconnects every account's connector, then rebuilds what the screen shows
 * from it (the room list, the open chat, tags and settings). It exists for the cases where sync has
 * stalled and new messages stop arriving until the app is restarted.
 */
class RefreshStore {
  busy = $state(false);
  error = $state<string | null>(null);

  async run() {
    if (this.busy) return;
    this.busy = true;
    this.error = null;
    try {
      expect(await client.invoke({ action: 'RefreshSync' }), 'SyncRefreshed');
      // Reopening the chat resets the composer's reply/edit state; carry it over so a refresh never eats it.
      const roomId = timelineStore.roomId;
      const { replyTo, editing } = timelineStore;
      await roomsStore.restart();
      if (roomId) {
        await timelineStore.open(roomId);
        timelineStore.replyTo = replyTo;
        timelineStore.editing = editing;
      }
      void tags.load();
      void prefs.load();
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      this.busy = false;
    }
  }
}

export const refreshStore = new RefreshStore();
