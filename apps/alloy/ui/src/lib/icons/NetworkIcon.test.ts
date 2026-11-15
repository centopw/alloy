import { describe, expect, it } from 'vitest';
import { mount, unmount } from 'svelte';
import NetworkIcon from './NetworkIcon.svelte';
import type { Network } from '$lib/generated';

function render(network: Network, label: string) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const component = mount(NetworkIcon, { target, props: { network, label } });
  return { target, done: () => (unmount(component), target.remove()) };
}

describe('NetworkIcon', () => {
  it.each(['telegram', 'messenger'] as const)('draws a named glyph for %s', (network) => {
    const { target, done } = render(network, network);
    const svg = target.querySelector('svg');
    expect(svg?.getAttribute('role')).toBe('img');
    expect(svg?.getAttribute('aria-label')).toBe(network);
    expect(svg?.querySelector('path')?.getAttribute('fill')).toBe('currentColor');
    done();
  });

  it('draws different glyphs for the two networks', () => {
    const a = render('telegram', 'a');
    const b = render('messenger', 'b');
    expect(a.target.querySelector('path')?.getAttribute('d')).not.toBe(b.target.querySelector('path')?.getAttribute('d'));
    a.done();
    b.done();
  });

  it('draws nothing for a network without a glyph', () => {
    const { target, done } = render('fake', 'Test network');
    expect(target.querySelector('svg')).toBeNull();
    done();
  });
});
