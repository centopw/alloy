import type { RoomTagDef, TagColour } from '../generated/index.js';
import { client } from '../ipc/index.js';
import { errorMessage, expect } from './errors.js';
import { registerFlushHandler } from './prefs.svelte.js';
import { makeTagId } from './tags.js';
/** The account's custom room tag catalogue, plus per-room assignment through the engine. */
class TagStore {
  defs = $state<RoomTagDef[]>([]);
  loaded = $state(false);
  error = $state<string | null>(null);
  #loadInflight: Promise<void> | null = null;
  #inflight = new Set<Promise<unknown>>();

  /** Forget everything (sign-out), so the next account never sees this one's tags. */
  reset() {
    this.defs = [];
    this.loaded = false;
    this.error = null;
  }

  /** Waits for all pending tag and catalogue writes to finish. */
  flush(): Promise<void> {
    return Promise.all([...this.#inflight]).then(() => undefined);
  }

  #track<T>(op: () => Promise<T>): Promise<T> {
    const p = op();
    const settled = p.then(() => undefined, () => undefined);
    this.#inflight.add(settled);
    void settled.finally(() => this.#inflight.delete(settled));
    return p;
  }

  /** Loads the catalogue; concurrent callers share the same request. */
  load(): Promise<void> {
    this.#loadInflight ??= this.#fetch().finally(() => {
      this.#loadInflight = null;
    });
    return this.#loadInflight;
  }

  async #fetch(): Promise<void> {
    try {
      const res = await client.invoke({ action: 'GetTagCatalog' });
      this.defs = expect(res, 'TagCatalog').payload.tags;
      this.loaded = true;
      this.error = null;
    } catch (e) {
      this.error = errorMessage(e);
    }
  }

  /** Replaces the whole catalogue. Returns false (and sets `error`) when the engine refuses. */
  #save(next: RoomTagDef[]): Promise<boolean> {
    return this.#track(async () => {
      try {
        expect(await client.invoke({ action: 'SaveTagCatalog', params: { tags: next } }), 'Unit');
        this.defs = next;
        this.error = null;
        return true;
      } catch (e) {
        this.error = errorMessage(e);
        return false;
      }
    });
  }

  /** Adds a tag at the end of the catalogue. Returns the new definition, or null on failure. */
  async create(name: string, colour: TagColour): Promise<RoomTagDef | null> {
    const trimmed = name.trim();
    if (trimmed === '') return null;
    const def: RoomTagDef = {
      id: makeTagId(trimmed, new Set(this.defs.map((d) => d.id))),
      name: trimmed,
      colour,
    };
    return (await this.#save([...this.defs, def])) ? def : null;
  }

  /** Renames and/or recolours a tag. The id never changes, so room assignments are kept. */
  async update(id: string, patch: { name?: string; colour?: TagColour }): Promise<boolean> {
    const name = patch.name?.trim();
    if (name === '') return false;
    const next = this.defs.map((d) =>
      d.id === id ? { ...d, name: name ?? d.name, colour: patch.colour ?? d.colour } : d,
    );
    return this.#save(next);
  }

  /** Moves a tag one step up (-1) or down (+1) in the display order. */
  async move(id: string, step: -1 | 1): Promise<boolean> {
    const from = this.defs.findIndex((d) => d.id === id);
    const to = from + step;
    if (from === -1 || to < 0 || to >= this.defs.length) return false;
    const next = [...this.defs];
    [next[from], next[to]] = [next[to], next[from]];
    return this.#save(next);
  }

  /** Deletes a tag from the catalogue and from every room that carries it. */
  remove(id: string): Promise<boolean> {
    return this.#track(async () => {
      try {
        expect(await client.invoke({ action: 'DeleteRoomTag', params: { tag_id: id } }), 'Unit');
        this.defs = this.defs.filter((d) => d.id !== id);
        this.error = null;
        return true;
      } catch (e) {
        this.error = errorMessage(e);
        return false;
      }
    });
  }

  /** Engine call that assigns or removes a tag on one room. Callers update the room list (see RoomsStore.setRoomTag). */
  setForRoom(roomId: string, tagId: string, enabled: boolean): Promise<boolean> {
    return this.#track(async () => {
      try {
        expect(
          await client.invoke({
            action: 'SetRoomTag',
            params: { room_id: roomId, tag_id: tagId, enabled },
          }),
          'Unit',
        );
        this.error = null;
        return true;
      } catch (e) {
        this.error = errorMessage(e);
        return false;
      }
    });
  }
}

export const tags = new TagStore();

registerFlushHandler(() => tags.flush());
