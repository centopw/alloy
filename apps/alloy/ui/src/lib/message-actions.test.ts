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
const storeSource = readFileSync(resolve(process.cwd(), 'src/lib/stores/timeline.svelte.ts'), 'utf8');

describe('message actions', () => {
  it('never loads a video file as its own poster image', () => {
    // GetMedia serves one file per media object: asking for a video "thumbnail" downloaded the
    // whole video, the <img> rejected it and every video showed "couldn't load".
    expect(mediaSource).not.toContain("video ? 'image/jpeg'");
    expect(mediaSource).toContain('const mime = visual ? mimeType : null;');
  });

  it('renders the toolbar inside the anchor sized to the message', () => {
    const anchorStart = rowSource.indexOf('<div class="message-anchor">');
    const anchorEnd = rowSource.indexOf('{#if seenAvatars}', anchorStart);
    const toolbar = rowSource.indexOf('<div class="toolbar"', anchorStart);
    expect(anchorStart, 'the row has a message anchor').toBeGreaterThanOrEqual(0);
    expect(anchorEnd, 'the anchor is closed before the seen row').toBeGreaterThan(anchorStart);
    expect(toolbar, 'the toolbar sits inside the anchor').toBeGreaterThan(anchorStart);
    expect(toolbar, 'the toolbar sits inside the anchor').toBeLessThan(anchorEnd);
  });

  it('caps the bubble against the row, never against the shrink-to-fit anchor', () => {
    // The anchor is `width: fit-content`, so a 72% cap left on the bubble resolves against the
    // anchor's own intrinsic width: every short message wrapped into a narrow column, rows grew
    // ~24px and the tall-window backfill stopped eight messages early (T18). The cap belongs on
    // the anchor, where the percentage resolves against the row.
    const anchor = rowSource.match(
      /\n {2}\.row\[data-layout='bubbles'\] \.message-anchor \{[^}]*\}/
    )?.[0];
    expect(anchor, 'the bubbles anchor has a rule').toBeTruthy();
    expect(anchor).toContain('width: fit-content;');
    expect(anchor).toContain('max-width: var(--alloy-bubble-max-width);');
    expect(rowSource).toContain(".row[data-layout='bubbles'] .message-anchor .bubble {");
    expect(rowSource).toMatch(
      /\.row\[data-layout='bubbles'\] \.message-anchor \.bubble \{[^}]*max-width: none;/
    );
  });

  it('gives pictures and albums a definite width so their bubble hugs them', () => {
    // A shrink-to-fit message box makes a percentage-width picture contribute its natural width,
    // so the bubble came out wider than the picture (or the album grid) and the content sat to one
    // side of it with the timestamp far away. Both caps must be definite; a percentage may only
    // appear in max-width, which shrinks them on a narrow screen.
    const frame = mediaSource.match(/\n {2}\.frame \{[^}]*\}/)?.[0];
    expect(frame, 'the picture frame has a rule').toBeTruthy();
    expect(frame).toContain('width: min(400px, calc(360px * var(--ratio, 1.3333)));');
    const album = rowSource.match(/\n {2}\.album-grid \{[^}]*\}/)?.[0];
    expect(album, 'the album grid has a rule').toBeTruthy();
    expect(album).toContain('width: 400px;');
    expect(album).toContain('max-width: 100%;');
    expect(album).not.toMatch(/width: min\([^;]*100%/);
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
    expect(rowSource).toContain('rowContentRight - gap - width - origin');
    expect(rowSource).toContain("addEventListener('mouseenter', show)");
  });

  it('derives the compact bar from the current width every time', () => {
    // A remembered `compact` left a row that was first hovered in a narrow pane compact after the
    // pane grew back: 96px of bar with 400px free.
    expect(rowSource).toContain('compact = false;\n    await flush();');
    expect(rowSource).toContain('if (bar.offsetWidth > available) {');
    expect(rowSource).not.toContain('if (!compact && bar.offsetWidth > available)');
  });

  it('keeps reply and an overflow in the compact bar', () => {
    // Asserted by message id: the English words live in the catalog, and pinning them here breaks the
    // test every time a string is extracted for translation.
    expect(rowSource).toContain("aria-label={t('message-row-more-actions')}");
    // The overflow opens the same menu right-click offers, so thread/edit/delete stay reachable.
    expect(rowSource).toContain('contextMenu.open(menuItems(), { x: rect.left, y: rect.bottom }, menuOptions())');
    expect(rowSource).toMatch(/\{:else\}[\s\S]*?aria-label=\{t\('message-row-more-actions'\)\}/);
  });

  it('floats the quick reactions over the message on desktop and leaves touch to the long-press menu', () => {
    // The reactions left the toolbar: they float above the bubble on hover/focus, inside the anchor so
    // they belong to the message, and only when the account supports reactions.
    const anchorStart = rowSource.indexOf('<div class="message-anchor">');
    const anchorEnd = rowSource.indexOf('{#if seenAvatars}', anchorStart);
    const float = rowSource.indexOf('<div class="quick-float"', anchorStart);
    expect(float).toBeGreaterThan(anchorStart);
    expect(float).toBeLessThan(anchorEnd);
    expect(rowSource).toContain('{#if canAct && canReact}');
    // One ToggleReaction path for every entry point: hover bar, long-press menu and double-tap.
    expect(rowSource.slice(float, anchorEnd)).toContain('onpick={reactWith}');
    expect(rowSource).toContain('onpick: reactWith,');
    expect(rowSource).toContain('.row:hover .quick-float,\n    .row:focus-within .quick-float {');
    expect(rowSource).toMatch(/@media \(pointer: coarse\) \{\s*\.quick-float \{\s*display: none;/);
    expect(rowSource).toMatch(/@media \(prefers-reduced-motion: reduce\) \{\s*\.quick-float,/);
    // The toolbar no longer carries a reaction strip.
    expect(rowSource).not.toContain('class="quick"');
    expect(rowSource).not.toContain('COMPACT_REACTIONS');
  });

  it('offers pin and unpin through SetMessagePinned', () => {
    expect(rowSource).toContain("id: 'pin', label: t('message-row-pin')");
    expect(rowSource).toContain("id: 'unpin', label: t('message-row-unpin')");
    expect(rowSource).toContain('timelineStore.setPinned(item, true)');
    expect(rowSource).toContain('timelineStore.setPinned(item, false)');
    // Unpinning never needs the capability; pinning does.
    expect(rowSource).toContain("(item.pinned || capabilities?.pin_messages === true)");
    expect(storeSource).toContain("action: 'SetMessagePinned'");
  });

  it('hides the bar while the context menu is open', () => {
    // The bar is laid out while hidden so it can measure itself; a hovered row's bar then sat over
    // the context menu's reaction strip and swallowed the click (E2E step at 0.3.0).
    expect(rowSource).toContain('class:hidden={contextMenu.isOpen}');
    expect(rowSource).toContain('.toolbar.hidden {');
  });

  it('pins the bar inside the row when even the compact bar does not fit', () => {
    expect(rowSource).toContain('const pinned = compact && width > available;');
    expect(rowSource).toContain('rowContentLeft + pinnedGap - origin');
    expect(rowSource).toContain('rowContentRight - pinnedGap - width - origin');
  });

  it('centres the bar on the message first line and animates without moving the placement', () => {
    expect(rowSource).toContain('const centre = targetRect2.top + paddingTop + line / 2;');
    expect(rowSource).toContain('toolbarTop = Math.round(centre - bar.offsetHeight / 2 - anchorRect.top);');
    expect(rowSource).toContain('top: var(--toolbar-top, calc(-1 * var(--alloy-spacing-scale-3)));');
    // The bar is laid out while hidden so its width can be measured, and fades rather than pops.
    expect(rowSource).toContain('visibility: hidden;');
    expect(rowSource).toContain('transition:\n      opacity 80ms cubic-bezier(0.4, 0, 1, 1),');
    expect(rowSource).toContain('@media (prefers-reduced-motion: reduce) {');
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
    expect(mediaSource).toContain("aria-label={t('media-download', { name: body || t('media-attachment') })}");
    expect(mediaSource).toContain('class="file-download"');
    expect(mediaSource).toContain('onclick={ondownload}');
  });

  it('is wired to the row download path rather than a second implementation', () => {
    expect(rowSource).toContain('ondownload={downloadMediaItem}');
    expect(rowSource).toContain('function downloadMediaItem()');
  });

  it('lifts the plate on hover instead of fading it', () => {
    // On an own bubble the resting plate is already 12% of the bubble text, so a low-opacity
    // currentColor hover would read as a dim instead of a lift.
    expect(mediaSource).toContain('background: var(--file-surface-hover');
    expect(rowSource).toContain('--file-surface-hover: color-mix');
  });
});

describe('audio tiles', () => {
  it('offers a labelled download control like a file tile', () => {
    expect(audioSource).toContain('class="download-btn"');
    expect(audioSource).toContain("aria-label={t('audio-player-download', { name: props.body || t('media-audio') })}");
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
