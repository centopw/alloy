import type { Prefs } from "../stores/prefs.svelte.js";

type StateVisibility = Pick<Prefs, "showMembershipEvents" | "showProfileChanges" | "showRoomUpdates">;

/** Human summary the engine attaches to a state event, or null when there is none. */
export function stateEventSummary(content: { content_json: string }): string | null {
  try {
    const parsed: unknown = JSON.parse(content.content_json);
    if (parsed && typeof parsed === "object" && "summary" in parsed) {
      const summary = parsed.summary;
      if (typeof summary === "string" && summary.length > 0) return summary;
    }
  } catch {
    // Malformed content: fall through to no summary.
  }
  return null;
}

/** Whether a state event of this type is hidden by the user's timeline prefs. */
export function isHiddenState(stateType: string, prefs: StateVisibility): boolean {
  switch (stateType) {
    case "membership":
      return !prefs.showMembershipEvents;
    case "profile":
      return !prefs.showProfileChanges;
    default:
      return !prefs.showRoomUpdates;
  }
}
