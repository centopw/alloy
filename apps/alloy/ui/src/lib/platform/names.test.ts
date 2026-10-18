import { describe, expect, it } from 'vitest';
import { displayNameFor, localpart } from './names';

describe('localpart', () => {
  it('returns the text between the sigil and the first colon', () => {
    expect(localpart('@alice:matrix.org')).toBe('alice');
    expect(localpart('@facebookbot:matrix.hce.vn')).toBe('facebookbot');
  });

  it('keeps everything before the first colon when the ID has extra colons', () => {
    expect(localpart('@a:b:c')).toBe('a');
  });

  it('tolerates a missing sigil', () => {
    expect(localpart('alice:matrix.org')).toBe('alice');
    expect(localpart('alice')).toBe('alice');
  });

  it('tolerates a missing domain', () => {
    expect(localpart('@alice')).toBe('alice');
    expect(localpart('@alice:')).toBe('alice');
  });

  it('returns an empty string when no localpart exists', () => {
    expect(localpart('')).toBe('');
    expect(localpart('   ')).toBe('');
    expect(localpart('@')).toBe('');
    expect(localpart('@:matrix.org')).toBe('');
    expect(localpart(':matrix.org')).toBe('');
  });

  it('trims surrounding whitespace', () => {
    expect(localpart('  @alice:matrix.org  ')).toBe('alice');
    expect(localpart('@ alice :matrix.org')).toBe('alice');
  });

  it('preserves unicode localparts', () => {
    expect(localpart('@ünï:example.org')).toBe('ünï');
    expect(localpart('@山田:example.org')).toBe('山田');
  });
});

describe('displayNameFor', () => {
  const ID = '@facebookbot:matrix.hce.vn';

  it('prefers the explicit display name', () => {
    expect(displayNameFor(ID, 'Facebook Bot', 'Member Name')).toBe('Facebook Bot');
  });

  it('falls back to the member display name when no explicit name is given', () => {
    expect(displayNameFor(ID, null, 'Member Name')).toBe('Member Name');
    expect(displayNameFor(ID, undefined, 'Member Name')).toBe('Member Name');
  });

  it('falls back to the localpart, never the full Matrix ID', () => {
    expect(displayNameFor(ID)).toBe('facebookbot');
    expect(displayNameFor(ID, null, null)).toBe('facebookbot');
    expect(displayNameFor(ID, '', '')).toBe('facebookbot');
  });

  it('treats whitespace-only names as absent', () => {
    expect(displayNameFor(ID, '   ', '\t')).toBe('facebookbot');
    expect(displayNameFor(ID, '  ', 'Member Name')).toBe('Member Name');
  });

  it('trims the names it returns', () => {
    expect(displayNameFor(ID, '  Alice  ')).toBe('Alice');
    expect(displayNameFor(ID, null, ' Bob ')).toBe('Bob');
  });

  it('keeps unicode display names intact', () => {
    expect(displayNameFor(ID, '李小龙')).toBe('李小龙');
    expect(displayNameFor(ID, 'Zoë Ångström')).toBe('Zoë Ångström');
  });

  it('uses a placeholder when the ID has no localpart', () => {
    expect(displayNameFor('@:matrix.org')).toBe('Unknown user');
    expect(displayNameFor('')).toBe('Unknown user');
    expect(displayNameFor('', null, '')).toBe('Unknown user');
  });
});
