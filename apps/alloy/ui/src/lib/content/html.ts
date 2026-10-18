import DOMPurify from 'dompurify';

const ALLOWED_TAGS = [
  'a','b','strong','i','em','u','s','del','code','pre','blockquote','p','br','ul','ol','li',
  'h1','h2','h3','h4','h5','h6','hr','span','sub','sup','table','thead','tbody','tr','th','td','details','summary',
];

/** Sanitize server-provided `formatted_body`. Links are forced to open externally. */
export function sanitizeHtml(html: string): string {
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR: ['href', 'title', 'class', 'start'],
    ALLOWED_URI_REGEXP: /^(?:https?:|mailto:)/i,
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
    FORBID_ATTR: ['style', 'onerror', 'onclick'],
  });
}
