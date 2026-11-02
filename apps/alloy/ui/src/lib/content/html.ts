import DOMPurify from 'dompurify';

const ALLOWED_TAGS = [
  'a','b','strong','i','em','u','s','del','code','pre','blockquote','p','br','ul','ol','li',
  'h1','h2','h3','h4','h5','h6','hr','span','sub','sup','table','thead','tbody','tr','th','td','details','summary',
];

/** Matrix spoilers: `<span data-mx-spoiler>` stays hidden until revealed. */
export const SPOILER_SELECTOR = 'span[data-mx-spoiler]';

/** The class a mention carries, in a body and in a quote alike. */
export const PILL_CLASS = 'alloy-pill';

/**
 * A Matrix mention as the servers send it: the identifier sits in the `matrix.to` fragment, either
 * plain (`#/@user:server`) or percent-encoded (`#/%40user%3Aserver`).
 */
const MATRIX_TO = /^https?:\/\/matrix\.to\/#\/[!#@%]/i;

// `ALLOW_DATA_ATTR: false` drops every data-* attribute; the spoiler marker is the one exception.
DOMPurify.addHook('uponSanitizeAttribute', (node, data) => {
  if (data.attrName === 'data-mx-spoiler' && node.nodeName === 'SPAN') data.forceKeepAttr = true;
});

// A name in a sentence that is really a person should read as one, so a mention is marked here and
// drawn as a chip by CSS. Marking it in the sanitizer means the body and a quoted message agree.
DOMPurify.addHook('afterSanitizeElements', (node) => {
  if (node.nodeName !== 'A') return;
  const anchor = node as Element;
  const href = anchor.getAttribute('href');
  if (href && MATRIX_TO.test(href)) anchor.classList.add(PILL_CLASS);
});

const BASE_OPTIONS = {
  ALLOWED_TAGS,
  ALLOWED_ATTR: ['href', 'title', 'class', 'start', 'data-mx-spoiler'],
  ALLOWED_URI_REGEXP: /^(?:https?:|mailto:)/i,
  ALLOW_DATA_ATTR: false,
  ALLOW_ARIA_ATTR: false,
  FORBID_ATTR: ['style', 'onerror', 'onclick'],
};

/** Sanitize server-provided `formatted_body`. Links are forced to open externally. */
export function sanitizeHtml(html: string): string {
  return DOMPurify.sanitize(html, BASE_OPTIONS);
}

/**
 * Inline tags only. An excerpt is clamped to two or three lines inside one text box, so a paragraph,
 * list or table from the original message would break both the markup and the clamp.
 */
const QUOTE_TAGS = [
  'a','b','strong','i','em','u','s','del','code','span','br','sub','sup',
];

/**
 * Sanitized HTML for a quoted message.
 *
 * A quote sits inside a button, and a link inside a button is an interactive control inside an
 * interactive control — unreachable by a keyboard and illegal HTML. So anchors keep their text and
 * their look but lose their target; a mention still reads as a pill. Block tags are dropped for the
 * same reason: the excerpt has to stay a run of inline text.
 */
export function sanitizeQuoteHtml(html: string): string {
  // Sanitize to a string first, then re-read it as DOM: what is walked here is already inert.
  const clean = DOMPurify.sanitize(html, { ...BASE_OPTIONS, ALLOWED_TAGS: QUOTE_TAGS });
  const host = document.createElement('div');
  host.innerHTML = clean;
  for (const anchor of Array.from(host.querySelectorAll('a'))) {
    const span = document.createElement('span');
    for (const attr of Array.from(anchor.attributes)) {
      if (attr.name !== 'href') span.setAttribute(attr.name, attr.value);
    }
    while (anchor.firstChild) span.appendChild(anchor.firstChild);
    anchor.replaceWith(span);
  }
  return host.innerHTML;
}
