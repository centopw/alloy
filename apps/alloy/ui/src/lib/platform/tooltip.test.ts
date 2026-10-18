import { describe, expect, it, beforeEach } from 'vitest';
import {
  accessibleName,
  placeTooltip,
  stashTitle,
  tooltipShortcut,
  tooltipText,
  TITLE_MIRROR_ATTR,
} from './tooltip';

function element(html: string): HTMLElement {
  const host = document.createElement('div');
  host.innerHTML = html;
  document.body.appendChild(host);
  return host.firstElementChild as HTMLElement;
}

describe('tooltipText', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('prefers data-tooltip over aria-label and title', () => {
    const el = element('<button data-tooltip="A" aria-label="B" title="C">x</button>');
    expect(tooltipText(el)).toBe('A');
  });

  it('falls back to aria-label, then title, then data-title', () => {
    expect(tooltipText(element('<button aria-label="B">x</button>'))).toBe('B');
    expect(tooltipText(element('<button title="C">x</button>'))).toBe('C');
    expect(tooltipText(element('<button data-title="D">x</button>'))).toBe('D');
  });

  it('returns null for non-interactive elements', () => {
    expect(tooltipText(element('<span aria-label="B">x</span>'))).toBeNull();
  });

  it('returns null when the text only repeats the visible label', () => {
    expect(tooltipText(element('<button aria-label="Send">Send</button>'))).toBeNull();
  });

  it('collapses whitespace and returns null for blank text', () => {
    expect(tooltipText(element('<button data-tooltip="  Two   words ">x</button>'))).toBe(
      'Two words',
    );
    expect(tooltipText(element('<button data-tooltip="   ">x</button>'))).toBeNull();
  });

  it('returns null inside the tooltip root', () => {
    const el = element('<div data-alloy-tooltip><button data-tooltip="A">x</button></div>');
    expect(tooltipText(el.querySelector('button') as HTMLElement)).toBeNull();
  });
});

describe('tooltipShortcut', () => {
  it('reads data-shortcut and trims it', () => {
    expect(tooltipShortcut(element('<button data-shortcut=" Ctrl+K ">x</button>'))).toBe('Ctrl+K');
    expect(tooltipShortcut(element('<button>x</button>'))).toBeNull();
  });
});

describe('accessibleName', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('uses aria-labelledby first', () => {
    document.body.innerHTML = '<span id="l">Label</span>';
    const el = element('<button aria-labelledby="l" aria-label="Other">x</button>');
    expect(accessibleName(el)).toBe('Label');
  });

  it('uses aria-label, then text content, then title', () => {
    expect(accessibleName(element('<button aria-label="B">x</button>'))).toBe('B');
    expect(accessibleName(element('<button>  Text </button>'))).toBe('Text');
    expect(accessibleName(element('<button title="T"></button>'))).toBe('T');
  });

  it('uses the associated label for inputs', () => {
    document.body.innerHTML = '<label for="c">Mute</label>';
    const el = element('<input id="c" type="checkbox">');
    expect(accessibleName(el)).toBe('Mute');
  });
});

describe('stashTitle', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('moves title to data-title and keeps the existing accessible name', () => {
    const el = element('<button aria-label="Mute">x</button>');
    el.setAttribute('title', 'Mute microphone');
    stashTitle(el);
    expect(el.hasAttribute('title')).toBe(false);
    expect(el.dataset.title).toBe('Mute microphone');
    expect(el.hasAttribute(TITLE_MIRROR_ATTR)).toBe(false);
    expect(el.getAttribute('aria-label')).toBe('Mute');
  });

  it('mirrors the title into aria-label when the element has no other name', () => {
    const el = element('<button></button>');
    el.setAttribute('title', 'Close');
    stashTitle(el);
    expect(el.dataset.title).toBe('Close');
    expect(el.getAttribute('aria-label')).toBe('Close');
    expect(el.hasAttribute(TITLE_MIRROR_ATTR)).toBe(true);
  });

  it('does nothing when there is no title', () => {
    const el = element('<button aria-label="A">x</button>');
    stashTitle(el);
    expect(el.dataset.title).toBeUndefined();
  });
});

describe('placeTooltip', () => {
  const viewport = { width: 800, height: 600 };
  const size = { width: 120, height: 30 };

  it('places above the anchor and centres horizontally', () => {
    const anchor = { left: 300, top: 200, bottom: 224, width: 40 };
    expect(placeTooltip(anchor, size, viewport)).toEqual({ x: 260, y: 164 });
  });

  it('falls back below the anchor when there is no room above', () => {
    const anchor = { left: 300, top: 10, bottom: 34, width: 40 };
    expect(placeTooltip(anchor, size, viewport).y).toBe(40);
  });

  it('clamps to the viewport edges', () => {
    const left = placeTooltip({ left: 0, top: 200, bottom: 224, width: 10 }, size, viewport);
    expect(left.x).toBe(8);
    const right = placeTooltip({ left: 790, top: 200, bottom: 224, width: 10 }, size, viewport);
    expect(right.x).toBe(800 - 120 - 8);
  });
});
