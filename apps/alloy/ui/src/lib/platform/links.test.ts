import { describe, expect, it } from 'vitest';
import { parseMatrixToLink } from './matrixTo';

describe('parseMatrixToLink', () => {
  it('parses user permalinks', () => {
    expect(parseMatrixToLink('https://matrix.to/#/@alice:matrix.org')).toEqual({
      kind: 'user',
      id: '@alice:matrix.org'
    });
  });

  it('parses room ids and room aliases', () => {
    expect(parseMatrixToLink('https://matrix.to/#/!abc123:example.org')).toEqual({
      kind: 'room',
      id: '!abc123:example.org'
    });
    expect(parseMatrixToLink('https://matrix.to/#/#general:example.org')).toEqual({
      kind: 'room',
      id: '#general:example.org'
    });
  });

  it('resolves event permalinks to their room', () => {
    expect(parseMatrixToLink('https://matrix.to/#/!abc123:example.org/$event1:example.org')).toEqual({
      kind: 'room',
      id: '!abc123:example.org'
    });
  });

  it('decodes percent-encoded identifiers', () => {
    expect(parseMatrixToLink('https://matrix.to/#/%40alice%3Amatrix.org')).toEqual({
      kind: 'user',
      id: '@alice:matrix.org'
    });
    expect(parseMatrixToLink('https://matrix.to/#/%23general%3Aexample.org')).toEqual({
      kind: 'room',
      id: '#general:example.org'
    });
  });

  it('ignores query parameters such as via servers', () => {
    expect(parseMatrixToLink('https://matrix.to/#/@alice:matrix.org?via=matrix.org')).toEqual({
      kind: 'user',
      id: '@alice:matrix.org'
    });
    expect(parseMatrixToLink('https://matrix.to/#/!abc:example.org?via=a.org&via=b.org')).toEqual({
      kind: 'room',
      id: '!abc:example.org'
    });
  });

  it('accepts http as well as https and trims surrounding whitespace', () => {
    expect(parseMatrixToLink('http://matrix.to/#/@alice:matrix.org')).toEqual({
      kind: 'user',
      id: '@alice:matrix.org'
    });
    expect(parseMatrixToLink('  https://matrix.to/#/@alice:matrix.org  ')).toEqual({
      kind: 'user',
      id: '@alice:matrix.org'
    });
  });

  it('rejects other hosts, including look-alikes', () => {
    expect(parseMatrixToLink('https://example.org/#/@alice:matrix.org')).toBeNull();
    expect(parseMatrixToLink('https://matrix.to.evil.example/#/@alice:matrix.org')).toBeNull();
    expect(parseMatrixToLink('https://evil.example/matrix.to/#/@alice:matrix.org')).toBeNull();
  });

  it('rejects non-web schemes', () => {
    expect(parseMatrixToLink('javascript:alert(1)')).toBeNull();
    expect(parseMatrixToLink('mailto:alice@matrix.org')).toBeNull();
  });

  it('rejects matrix.to links without a fragment target', () => {
    expect(parseMatrixToLink('https://matrix.to/')).toBeNull();
    expect(parseMatrixToLink('https://matrix.to/#/')).toBeNull();
    expect(parseMatrixToLink('https://matrix.to/#alice')).toBeNull();
  });

  it('rejects identifiers that are not user, room or alias ids', () => {
    expect(parseMatrixToLink('https://matrix.to/#/alice:matrix.org')).toBeNull();
    expect(parseMatrixToLink('https://matrix.to/#/@alice')).toBeNull();
    expect(parseMatrixToLink('https://matrix.to/#/@:matrix.org')).toBeNull();
    expect(parseMatrixToLink('https://matrix.to/#/!room:')).toBeNull();
  });

  it('returns null for strings that are not URLs', () => {
    expect(parseMatrixToLink('')).toBeNull();
    expect(parseMatrixToLink('not a url')).toBeNull();
  });

  it('returns null for malformed percent-encoding', () => {
    expect(parseMatrixToLink('https://matrix.to/#/%E0%A4%A')).toBeNull();
  });
});
