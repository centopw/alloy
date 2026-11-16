/**
 * App-level UI state shared between the shell and the settings menu. Everything here is
 * device-local: which panel is open, never anything a network knows about.
 */
export type RoomInfoTab = 'info' | 'media' | 'files' | 'links' | 'members' | 'search';

class UiStore {
  /** Developer HUD overlay (dev builds only). */
  hudVisible = $state(false);
  /** Room info panel (details, shared media, notes, search) on the right of the conversation. */
  roomInfoOpen = $state(false);
  roomInfoTab = $state<RoomInfoTab>('info');
  /** In-room message search (side slot). Synchronized with `roomInfoTab === 'search'`. */
  roomSearchOpen = $state(false);

  /** Opens the room info panel for the active room on `tab`. */
  openRoomInfo(tab: RoomInfoTab = 'info') {
    this.roomInfoTab = tab;
    this.roomInfoOpen = true;
    this.roomSearchOpen = tab === 'search';
  }

  closeRoomInfo() {
    this.roomInfoOpen = false;
    this.roomSearchOpen = false;
  }

  toggleRoomInfo(tab: RoomInfoTab = 'info') {
    if (this.roomInfoOpen && this.roomInfoTab === tab) {
      this.closeRoomInfo();
    } else {
      this.openRoomInfo(tab);
    }
  }

  /** Opens the in-room search tab in the right-hand inspector drawer. */
  openRoomSearch() {
    this.openRoomInfo('search');
  }

  closeRoomSearch() {
    this.roomSearchOpen = false;
    if (this.roomInfoTab === 'search') {
      this.roomInfoOpen = false;
    }
  }

  toggleRoomSearch() {
    if (this.roomSearchOpen) this.closeRoomSearch();
    else this.openRoomSearch();
  }
}

export const ui = new UiStore();
