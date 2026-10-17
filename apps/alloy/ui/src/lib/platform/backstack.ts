/**
 * Android's system Back button navigates WebView history and leaves the app when there is none.
 * Overlays (dialogs, the open chat on phones) register here so Back closes the topmost one instead.
 */
type Entry = { handler: () => void };

const stack: Entry[] = [];
let programmaticPops = 0;
let listening = false;

function onPopState() {
  if (programmaticPops > 0) {
    programmaticPops--;
    return;
  }
  stack.pop()?.handler();
}

/** Back only exists as a gesture/button on phone-sized layouts; desktop keeps plain overlays. */
export function backStackEnabled(): boolean {
  return typeof window !== 'undefined' && window.innerWidth < 900;
}

/**
 * Make the system Back button call `handler` (once). Returns a release function to call when the
 * overlay closes by other means; it removes the history entry again.
 */
export function pushBack(handler: () => void): () => void {
  if (!backStackEnabled()) return () => {};
  if (!listening) {
    window.addEventListener('popstate', onPopState);
    listening = true;
  }
  const entry: Entry = { handler };
  stack.push(entry);
  history.pushState({ alloyBack: true }, '');
  return () => {
    const i = stack.indexOf(entry);
    if (i < 0) return; // already consumed by Back
    const wasTop = i === stack.length - 1;
    stack.splice(i, 1);
    if (wasTop) {
      programmaticPops++;
      history.back();
    }
  };
}
