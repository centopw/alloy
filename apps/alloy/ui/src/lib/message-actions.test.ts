import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * Guard for two Element-parity fixes (#114): the message toolbar must belong to the message, and a
 * file tile must carry its own download control. Both were "present but in the wrong place or
 * invisible", which no unit test noticed, so this reads the components the way a reviewer would.
 */

const rowSource = readFileSync(
  resolve(process.cwd(), 'src/lib/components/timeline/MessageRow.svelte'),
  'utf8'
);
const mediaSource = readFileSync(
  resolve(process.cwd(), 'src/lib/components/timeline/MediaView.svelte'),
  'utf8'
);

describe('message actions', () => {
  it('renders the toolbar inside the anchor sized to the message', () => {
    const anchorStart = rowSource.indexOf('<div class="message-anchor">');
    const anchorEnd = rowSource.indexOf('{#if seenAvatars}', anchorStart);
    const toolbar = rowSource.indexOf('<div class="toolbar"', anchorStart);
    expect(anchorStart, 'the row has a message anchor').toBeGreaterThanOrEqual(0);
    expect(anchorEnd, 'the anchor is closed before the seen row').toBeGreaterThan(anchorStart);
    expect(toolbar, 'the toolbar sits inside the anchor').toBeGreaterThan(anchorStart);
    expect(toolbar, 'the toolbar sits inside the anchor').toBeLessThan(anchorEnd);
  });

  it('places the toolbar beside the bubble, on the inner side of each message', () => {
    const incoming = rowSource.match(
      /\n {2}\.row\[data-layout='bubbles'\] \.toolbar \{[^}]*\}/
    )?.[0];
    const own = rowSource.match(/\n {2}\.row\.own\[data-layout='bubbles'\] \.toolbar \{[^}]*\}/)?.[0];
    expect(incoming, 'the bubbles toolbar has a rule').toBeTruthy();
    expect(own, 'own messages have their own rule').toBeTruthy();
    // Incoming bubbles are left-aligned, so the toolbar comes after them; own messages mirror it.
    expect(incoming).toContain('left: calc(100%');
    expect(incoming).toContain('right: auto');
    expect(own).toContain('right: calc(100%');
    expect(own).toContain('left: auto');
  });
});

describe('file tiles', () => {
  it('offers a labelled download control', () => {
    expect(mediaSource).toContain('aria-label="Download {body || \'attachment\'}"');
    expect(mediaSource).toContain('class="file-download"');
    expect(mediaSource).toContain('onclick={ondownload}');
  });

  it('is wired to the row download path rather than a second implementation', () => {
    expect(rowSource).toContain('ondownload={downloadMediaItem}');
    expect(rowSource).toContain('function downloadMediaItem()');
  });
});
