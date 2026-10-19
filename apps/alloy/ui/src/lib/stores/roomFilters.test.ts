import { describe, it, expect } from 'vitest';
import { ROOM_FILTERS, type RoomFilter } from './rooms.svelte';

describe('ROOM_FILTERS', () => {
  it('lists every room filter exactly once, in chip order', () => {
    const values = ROOM_FILTERS.map((f) => f.value);
    const all: RoomFilter[] = ['all', 'direct', 'rooms', 'unread', 'archive'];
    expect([...values].sort()).toEqual([...all].sort());
    expect(new Set(values).size).toBe(values.length);
    expect(values[0]).toBe('all');
  });

  it('labels the Channels chip for the rooms filter', () => {
    expect(ROOM_FILTERS.map((f) => f.label)).toEqual(['All', 'Direct', 'Channels', 'Unread', 'Archive']);
  });
});
