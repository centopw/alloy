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
const audioSource = readFileSync(
  resolve(process.cwd(), 'src/lib/components/timeline/AudioPlayer.svelte'),
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
    // The measured --toolbar-left/--toolbar-right wins once the row has hovered; the calc() is the
    // first-paint fallback for the same geometry.
    expect(incoming).toContain('left: var(--toolbar-left, calc(100%');
    expect(incoming).toContain('right: var(--toolbar-right, auto)');
    expect(own).toContain('right: var(--toolbar-right, calc(100%');
    expect(own).toContain('left: var(--toolbar-left, auto)');
  });

  it('measures the bar against the message and clamps it inside the row', () => {
    // Placement is a measurement, not a constant: the anchor can be wider than a wrapped bubble,
    // and a row-filling message leaves no room outside its content.
    expect(rowSource).toContain('function placeToolbar');
    expect(rowSource).toContain("layout === 'bubbles' ? (bubble?.getBoundingClientRect() ?? anchorRect) : anchorRect");
    expect(rowSource).toContain('rowContentRight - gap - bar.offsetWidth - origin');
    expect(rowSource).toContain("addEventListener('mouseenter', show)");
  });

  it('drops the quick-reaction group before clamping over the message', () => {
    // The bar is one fixed width with quick reactions; on an own message beside a wide bubble that
    // width does not fit, and clamping alone would cover the text instead of clipping it.
    expect(rowSource).toContain('if (!compact && bar.offsetWidth > available)');
    expect(rowSource).toMatch(/\{#if !compact\}[\s\S]*?<QuickReactions/);
    expect(rowSource).toContain('class="quick"');
    expect(rowSource).toContain('.quick {\n    display: contents;');
  });

  it('never shadows the Svelte scheduler with the read-tick snippet', () => {
    // The collapse awaits the scheduler to flush the narrow bar. A snippet named `tick` shadows that
    // import in the render scope, so the first collapse throws `invalid_snippet_arguments` and the
    // bar silently stays at the CSS fallback position.
    expect(rowSource).toContain("import { tick as flush } from 'svelte'");
    expect(rowSource).toContain('await flush()');
    expect(rowSource).toContain('{#snippet readTicks()}');
    expect(rowSource).not.toContain('{#snippet tick()}');
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

describe('audio tiles', () => {
  it('offers a labelled download control like a file tile', () => {
    expect(audioSource).toContain('class="download-btn"');
    expect(audioSource).toContain('aria-label="Download {props.body || \'audio\'}"');
    expect(audioSource).toContain('onclick={props.ondownload}');
  });

  it('is wired to the same row download path', () => {
    const audioStart = rowSource.indexOf('<AudioPlayer');
    const audioEnd = rowSource.indexOf('/>', audioStart);
    expect(audioStart, 'the row renders an audio player').toBeGreaterThanOrEqual(0);
    expect(audioEnd, 'the audio player element closes').toBeGreaterThan(audioStart);
    expect(rowSource.slice(audioStart, audioEnd)).toContain('ondownload={downloadMediaItem}');
  });
});
