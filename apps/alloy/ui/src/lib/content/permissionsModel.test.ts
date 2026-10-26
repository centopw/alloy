import { describe, expect, it } from 'vitest';
import { actionFor, shouldPrompt, specsFor, stateLabel, summarize } from './permissionsModel';
import type { PermissionState } from '../platform/permissions';

const row = (required: boolean, state: PermissionState) => ({ required, state });

describe('specsFor', () => {
  it('hides the Android only permissions elsewhere', () => {
    expect(specsFor(false).map((s) => s.id)).toEqual(['notifications', 'microphone', 'location']);
  });

  it('lists the push app and battery on Android, with notifications as the only required one', () => {
    const android = specsFor(true);
    expect(android.map((s) => s.id)).toEqual(['notifications', 'push-app', 'battery', 'microphone', 'location']);
    expect(android.filter((s) => s.required).map((s) => s.id)).toEqual(['notifications']);
  });
});

describe('actionFor', () => {
  it('offers Allow until the user has decided', () => {
    expect(actionFor('prompt', false)).toBe('allow');
    expect(actionFor('unknown', false)).toBe('allow');
  });

  it('sends a refusal to the system settings, and offers nothing where there is no such page', () => {
    expect(actionFor('denied', true)).toBe('open-settings');
    expect(actionFor('denied', false)).toBeNull();
  });

  it('offers nothing once allowed or when the device cannot grant it', () => {
    expect(actionFor('granted', true)).toBeNull();
    expect(actionFor('unavailable', true)).toBeNull();
  });
});

describe('stateLabel', () => {
  it('reads "Not asked yet" for both undecided states', () => {
    expect(stateLabel('prompt')).toBe('Not asked yet');
    expect(stateLabel('unknown')).toBe('Not asked yet');
    expect(stateLabel('granted')).toBe('Allowed');
    expect(stateLabel('denied')).toBe('Not allowed');
  });
});

describe('summarize', () => {
  it('counts only required permissions, and ignores one the device cannot grant', () => {
    const s = summarize([row(true, 'granted'), row(true, 'denied'), row(false, 'denied'), row(true, 'unavailable')]);
    expect(s).toMatchObject({ missingRequired: 1, requiredTotal: 2, text: '1 of 2 required permissions allowed' });
  });

  it('says everything is allowed when no required permission is missing', () => {
    expect(summarize([row(true, 'granted'), row(false, 'prompt')]).text).toBe('Everything Alloy needs is allowed.');
  });

  it('says so when nothing is required on the device', () => {
    expect(summarize([row(true, 'unavailable')]).text).toBe('Nothing here is required on this device.');
  });
});

describe('shouldPrompt', () => {
  it('prompts while a required permission is missing', () => {
    expect(shouldPrompt([row(true, 'prompt')], false)).toBe(true);
    expect(shouldPrompt([row(true, 'denied')], false)).toBe(true);
  });

  it('does not prompt once the user dismissed it, even if the permission is still missing', () => {
    expect(shouldPrompt([row(true, 'denied')], true)).toBe(false);
  });

  it('does not prompt when only optional permissions are missing', () => {
    expect(shouldPrompt([row(true, 'granted'), row(false, 'denied')], false)).toBe(false);
  });

  it('does not prompt for a permission the device cannot grant', () => {
    expect(shouldPrompt([row(true, 'unavailable')], false)).toBe(false);
  });
});
