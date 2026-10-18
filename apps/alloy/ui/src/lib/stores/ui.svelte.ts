export type RoomInfoTab = 'info' | 'media' | 'files' | 'links' | 'members';

/** App-level UI state shared between the shell and the settings menu. */
class UiStore {
  /** Developer HUD overlay (dev builds only). */
  hudVisible = $state(false);
  /** Members panel on the right of the conversation. */
  membersOpen = $state(typeof window === 'undefined' || window.innerWidth >= 1100);
  /** Which room-sidebar scope is selected: the home view or one space id. */
  scope = $state<'home' | string>('home');
  /** Room info panel (details, shared media, notes) on the right of the conversation. */
  roomInfoOpen = $state(false);
  roomInfoTab = $state<RoomInfoTab>('info');

  /** Opens the room info panel for the active room on `tab`; it replaces the members panel. */
  openRoomInfo(tab: RoomInfoTab = 'info') {
    this.roomInfoTab = tab;
    this.roomInfoOpen = true;
  }

  closeRoomInfo() {
    this.roomInfoOpen = false;
  }

  toggleRoomInfo() {
    if (this.roomInfoOpen) this.closeRoomInfo();
    else this.openRoomInfo();
  }
}

export const ui = new UiStore();
