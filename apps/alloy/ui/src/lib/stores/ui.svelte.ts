/** App-level UI state shared between the shell and the settings menu. */
class UiStore {
  /** Developer HUD overlay (dev builds only). */
  hudVisible = $state(false);
  /** Members panel on the right of the conversation. */
  membersOpen = $state(true);
  /** Which room-sidebar scope is selected: the home view or one space id. */
  scope = $state<'home' | string>('home');
}

export const ui = new UiStore();
