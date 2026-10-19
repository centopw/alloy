import { client } from '../ipc/index.js';
import { expect } from './errors.js';

/** Unsent message text per room: shown as "Draft:" in the room list and restored in the composer. */
class DraftsStore {
  private previews = $state<Record<string, string>>({});
  private timers = new Map<string, ReturnType<typeof setTimeout>>();

  /** The saved draft text of a room for the list preview, or null. */
  previewOf(roomId: string): string | null {
    return this.previews[roomId] ?? null;
  }

  /** Loads the draft previews of every room (after sign-in or a restart). */
  async loadAll(): Promise<void> {
    try {
      const res = expect(await client.listDrafts(), 'Drafts');
      this.previews = Object.fromEntries(res.payload.items.map((d) => [d.room_id, d.text]));
    } catch {
      /* previews are a convenience; the composer still restores drafts per room */
    }
  }

  /** The room's saved draft, or null. */
  async load(roomId: string): Promise<{ text: string; replyTo: string | null; editOf: string | null } | null> {
    try {
      const res = expect(await client.getDraft(roomId), 'Draft');
      const draft = res.payload.draft;
      return draft ? { text: draft.text, replyTo: draft.reply_to, editOf: draft.edit_of } : null;
    } catch {
      return null;
    }
  }

  /** Saves a room's draft 500 ms after the last change; an empty draft clears it. */
  save(roomId: string, text: string, opts?: { replyTo?: string | null; editOf?: string | null }): void {
    this.#setPreview(roomId, text);
    clearTimeout(this.timers.get(roomId));
    this.timers.set(roomId, setTimeout(() => void this.flush(roomId, text, opts), 500));
  }

  /** Saves now when a save is still waiting for its timer (leaving a room, closing the app). */
  flushPending(roomId: string, text: string, opts?: { replyTo?: string | null; editOf?: string | null }): void {
    if (this.timers.has(roomId)) void this.flush(roomId, text, opts);
  }

  /** Saves at once, for leaving a room before the timer fires. */
  async flush(roomId: string, text: string, opts?: { replyTo?: string | null; editOf?: string | null }): Promise<void> {
    clearTimeout(this.timers.get(roomId));
    this.timers.delete(roomId);
    try {
      await client.saveDraft(roomId, text, { reply_to: opts?.replyTo ?? null, edit_of: opts?.editOf ?? null });
    } catch {
      /* a draft that cannot be stored is lost silently; the user keeps typing */
    }
  }

  #setPreview(roomId: string, text: string) {
    if (text.trim() === '') delete this.previews[roomId];
    else this.previews[roomId] = text;
  }

  reset() {
    for (const t of this.timers.values()) clearTimeout(t);
    this.timers.clear();
    this.previews = {};
  }
}

export const draftsStore = new DraftsStore();
