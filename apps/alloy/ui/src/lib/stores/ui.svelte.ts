/**
 * App-level UI state shared between the shell and the settings menu. Everything here is
 * device-local: which panel is open, never anything a network knows about.
 */
export type RoomInfoTab = 'info' | 'media' | 'files' | 'links' | 'members';

class UiStore {
  /** Developer HUD overlay (dev builds only). */
  hudVisible = $state(false);
  /** Room info panel (details, shared media, notes) on the right of the conversation. */
  roomInfoOpen = $state(false);
  roomInfoTab = $state<RoomInfoTab>('info');
  /** In-room message search (side slot). Exclusive with the room info panel. */
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

  /** Opens the in-room search panel. It takes the side slot and closes room info. */
  openRoomSearch() {
    this.roomInfoOpen = false;
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
