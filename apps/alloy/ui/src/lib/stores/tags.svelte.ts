import type { RoomTagDef, TagColour } from '../generated/index.js';
import { client } from '../ipc/index.js';
import { errorMessage, expect } from './errors.js';
import { registerFlushHandler } from './prefs.svelte.js';
import { makeTagId, refuseCatalogueWrite } from './tags.js';
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
      const next = expect(res, 'TagCatalog').payload.tags;
      // An empty answer is also what a catalogue the server never stored looks like, so it cannot
      // be used to erase a list we already have: the chips and their filter would vanish until the
      // next read. Deleting the last tag in the app empties `defs` first, so that case still lands.
      if (next.length === 0 && this.defs.length > 0) {
        console.warn(
          `[tags] GetTagCatalog returned no tags while ${this.defs.length} are loaded; keeping the local catalogue`
        );
        this.loaded = true;
        this.error = null;
        return;
      }
      this.defs = next;
      this.loaded = true;
      this.error = null;
    } catch (e) {
      this.error = errorMessage(e);
    }
  }

  /** Replaces the whole catalogue. Returns false (and sets `error`) when the engine refuses. */
  #save(next: RoomTagDef[], op: string): Promise<boolean> {
    return this.#track(async () => {
      const refusal = refuseCatalogueWrite(op, { loaded: this.loaded, next: next.length });
      if (refusal) {
        console.warn(refusal);
        return false;
      }
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
    return (await this.#save([...this.defs, def], 'create')) ? def : null;
  }

  /** Renames and/or recolours a tag. The id never changes, so room assignments are kept. */
  async update(id: string, patch: { name?: string; colour?: TagColour }): Promise<boolean> {
    const name = patch.name?.trim();
    if (name === '') return false;
    // An orphan chip (a room tag with no catalogue entry) has nothing to rename: writing the local
    // list back would drop every entry this client has not read yet.
    if (!this.defs.some((d) => d.id === id)) {
      console.warn(`[tags] refusing to update ${id}: it is not in the catalogue`);
      return false;
    }
    const next = this.defs.map((d) =>
      d.id === id ? { ...d, name: name ?? d.name, colour: patch.colour ?? d.colour } : d,
    );
    return this.#save(next, 'update');
  }

  /** Moves a tag one step up (-1) or down (+1) in the display order. */
  async move(id: string, step: -1 | 1): Promise<boolean> {
    const from = this.defs.findIndex((d) => d.id === id);
    const to = from + step;
    if (from === -1 || to < 0 || to >= this.defs.length) return false;
    const next = [...this.defs];
    [next[from], next[to]] = [next[to], next[from]];
    return this.#save(next, 'move');
  }

  /** Deletes a tag from the catalogue and from every room that carries it. */
  remove(id: string): Promise<boolean> {
    // The engine re-reads the catalogue and rewrites it without this tag, so an id the catalogue
    // does not know would turn the rewrite into "drop everything else too".
    if (!this.defs.some((d) => d.id === id)) {
      console.warn(`[tags] refusing to remove ${id}: it is not in the catalogue`);
      return Promise.resolve(false);
    }
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
