import type { RoomTagDef, TagColour } from '../generated/index.js';
import { client } from '../ipc/index.js';
import { errorMessage, expect } from './errors.js';
import { registerFlushHandler } from './prefs.svelte.js';
import { refuseTagWrite } from './tags.js';
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

  /** Adds one tag through the engine, which assigns the id. Returns the stored definition. */
  async create(name: string, colour: TagColour): Promise<RoomTagDef | null> {
    const trimmed = name.trim();
    if (trimmed === '') return null;
    const refusal = refuseTagWrite('create', this.loaded);
    if (refusal) {
      console.warn(refusal);
      return null;
    }
    return this.#track(async () => {
      try {
        const res = await client.invoke({ action: 'CreateRoomTag', params: { name: trimmed, colour } });
        const tag = { ...expect(res, 'RoomTagCreated').payload.tag };
        this.defs = [...this.defs, tag];
        this.error = null;
        return tag;
      } catch (e) {
        this.error = errorMessage(e);
        return null;
      }
    });
  }

  /** Renames and/or recolours one tag; the engine refuses an id the catalogue does not hold. */
  async update(id: string, patch: { name?: string; colour?: TagColour }): Promise<boolean> {
    const name = patch.name?.trim();
    if (name === '') return false;
    // An orphan chip (a room tag with no catalogue entry) has nothing to rename, and a rename sent
    // before the catalogue was read would name an id this client has not seen yet.
    const refusal = refuseTagWrite(`update ${id}`, this.loaded, this.defs.some((d) => d.id === id));
    if (refusal) {
      console.warn(refusal);
      return false;
    }
    return this.#track(async () => {
      try {
        expect(
          await client.invoke({
            action: 'RenameRoomTag',
            params: { id, name: name ?? null, colour: patch.colour ?? null },
          }),
          'Unit',
        );
        this.defs = this.defs.map((d) =>
          d.id === id ? { ...d, name: name ?? d.name, colour: patch.colour ?? d.colour } : d,
        );
        this.error = null;
        return true;
      } catch (e) {
        this.error = errorMessage(e);
        return false;
      }
    });
  }

  /** Moves a tag one step up (-1) or down (+1). The engine validates the order as a permutation. */
  async move(id: string, step: -1 | 1): Promise<boolean> {
    const from = this.defs.findIndex((d) => d.id === id);
    const to = from + step;
    if (from === -1 || to < 0 || to >= this.defs.length) return false;
    const refusal = refuseTagWrite(`move ${id}`, this.loaded, true);
    if (refusal) {
      console.warn(refusal);
      return false;
    }
    const ids = this.defs.map((d) => d.id);
    [ids[from], ids[to]] = [ids[to], ids[from]];
    return this.#track(async () => {
      try {
        expect(await client.invoke({ action: 'ReorderRoomTags', params: { ids } }), 'Unit');
        const byId = new Map(this.defs.map((d) => [d.id, d] as const));
        this.defs = ids.map((tagId) => byId.get(tagId)!);
        this.error = null;
        return true;
      } catch (e) {
        this.error = errorMessage(e);
        return false;
      }
    });
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
