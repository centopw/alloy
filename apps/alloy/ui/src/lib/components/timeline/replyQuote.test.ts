import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * T33: the reply quote and the reply bubble were glued into one column with square junction
 * corners, so a short reply beside a long quote opened a notch. They are separate cards now, with
 * the quoted name moved out of the quote into a header line above it.
 */

const quote = readFileSync(resolve(process.cwd(), 'src/lib/components/timeline/ReplyQuote.svelte'), 'utf8');
const row = readFileSync(resolve(process.cwd(), 'src/lib/components/timeline/MessageRow.svelte'), 'utf8');

describe('reply quote', () => {
  it('names the quoted message in a header above the card, not inside it', () => {
    expect(quote).toContain('Reply to <span class="reply-header-name">{senderName || \'user\'}</span>');
    // The name is rendered in the card only for the inline (flat layout) quote.
    expect(quote).toContain('{#if senderName && !stacked}');
    expect(quote.match(/\{#if senderName && !stacked\}/g)).toHaveLength(2);
  });

  it('draws the sender colour as an inset spine, clear of the radius', () => {
    expect(quote).toContain('.reply-quote.stacked::before');
    expect(quote).toContain('width: 2px;');
    expect(quote).not.toContain('border-left: 3px solid var(--quote-border-color, var(--alloy-color-border-prominent));\n    border-radius: var(--alloy-bubble-radius) var(--alloy-bubble-radius) 0 0;');
  });

  it('gives the card a full radius and no glued column', () => {
    const stacked = quote.slice(quote.indexOf('.reply-quote.stacked {'), quote.indexOf('/* On the saturated own bubble'));
    expect(stacked).toContain('border-radius: var(--alloy-bubble-radius);');
    expect(stacked).toContain('width: fit-content;');
    expect(quote).not.toContain('class="quote-stack"');
  });

  it('clamps the stacked excerpt to three lines', () => {
    expect(quote).toContain('.reply-quote.stacked .excerpt {');
    expect(quote).toMatch(/\.reply-quote\.stacked \.excerpt \{[^}]*-webkit-line-clamp: 3;/);
  });

  it('stacks header, card and bubble as separate surfaces on the sender side', () => {
    expect(row).toContain('gap: var(--alloy-spacing-scale-1);');
    expect(row).toContain('.row.own.replied[data-layout=\'bubbles\'] .message-stack {\n    align-items: flex-end;');
    // No square junction: the reply bubble keeps all four corners.
    expect(row).not.toContain('.row.replied[data-layout=\'bubbles\'] .bubble,\n  .row.replied.grouped[data-layout=\'bubbles\'] .bubble {\n    border-top-left-radius: 0;');
    expect(row).toContain('.row.replied.grouped[data-layout=\'bubbles\'] .bubble {\n    border-top-left-radius: var(--alloy-bubble-radius);');
  });
});
