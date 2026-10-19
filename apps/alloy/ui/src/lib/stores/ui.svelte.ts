import { threadStore } from './thread.svelte.js';

export type RoomInfoTab = 'info' | 'media' | 'files' | 'links' | 'members';

/** App-level UI state shared between the shell and the settings menu. */
class UiStore {
  /** Developer HUD overlay (dev builds only). */
  hudVisible = $state(false);
  /** Which room-sidebar scope is selected: the home view or one space id. */
  scope = $state<'home' | string>('home');
  /** Room info panel (details, shared media, notes) on the right of the conversation. */
  roomInfoOpen = $state(false);
  roomInfoTab = $state<RoomInfoTab>('info');
  /** In-room message search (side slot). Exclusive with room info and threads. */
  roomSearchOpen = $state(false);

  /** Opens the room info panel for the active room on `tab`. */
  openRoomInfo(tab: RoomInfoTab = 'info') {
    this.roomSearchOpen = false;
    this.roomInfoTab = tab;
    this.roomInfoOpen = true;
  }

  closeRoomInfo() {
    this.roomInfoOpen = false;
  }

  toggleRoomInfo(tab: RoomInfoTab = 'info') {
    if (this.roomInfoOpen && this.roomInfoTab === tab) {
      this.closeRoomInfo();
    } else {
      this.openRoomInfo(tab);
    }
  }

  /** Opens the in-room search panel. It takes the side slot and closes room info and any thread. */
  openRoomSearch() {
    this.roomInfoOpen = false;
    if (threadStore.roomId) void threadStore.close();
    this.roomSearchOpen = true;
  }

  closeRoomSearch() {
    this.roomSearchOpen = false;
  }

  toggleRoomSearch() {
    if (this.roomSearchOpen) this.closeRoomSearch();
    else this.openRoomSearch();
  }
}

export const ui = new UiStore();
