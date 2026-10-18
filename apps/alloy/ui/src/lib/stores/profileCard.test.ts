import { describe, expect, it } from 'vitest';
import { ProfileCardStore } from './profileCard.svelte';

describe('ProfileCardStore', () => {
  it('starts closed with no card data', () => {
    const card = new ProfileCardStore();

    expect(card.isOpen).toBe(false);
    expect(card.userId).toBeNull();
    expect(card.roomId).toBeNull();
    expect(card.at).toBeNull();
  });

  it('opens a card for a user at an anchor point', () => {
    const card = new ProfileCardStore();
    card.open('@bob:alloy.chat', '!general:alloy.chat', { x: 10, y: 20 });

    expect(card.isOpen).toBe(true);
    expect(card.userId).toBe('@bob:alloy.chat');
    expect(card.roomId).toBe('!general:alloy.chat');
    expect(card.at).toEqual({ x: 10, y: 20 });
  });

  it('copies the anchor so later changes to the caller object do not leak in', () => {
    const card = new ProfileCardStore();
    const anchor = { x: 1, y: 2 };
    card.open('@bob:alloy.chat', null, anchor);
    anchor.x = 99;

    expect(card.at).toEqual({ x: 1, y: 2 });
  });

  it('replaces a card that is already open', () => {
    const card = new ProfileCardStore();
    card.open('@bob:alloy.chat', '!general:alloy.chat', { x: 1, y: 1 });
    card.open('@carol:alloy.chat', null, { x: 5, y: 6 });

    expect(card.userId).toBe('@carol:alloy.chat');
    expect(card.roomId).toBeNull();
    expect(card.at).toEqual({ x: 5, y: 6 });
  });

  it('close resets every field', () => {
    const card = new ProfileCardStore();
    card.open('@bob:alloy.chat', '!general:alloy.chat', { x: 1, y: 2 });
    card.close();

    expect(card.isOpen).toBe(false);
    expect(card.userId).toBeNull();
    expect(card.roomId).toBeNull();
    expect(card.at).toBeNull();
  });
});
