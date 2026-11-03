import { describe, expect, it } from 'vitest';
import { resolve } from 'node:path';
import {
  copyKeyProblems,
  hardcodedCopy,
  messageIds,
  moduleCopy,
  moduleExcluded,
  scriptCopy,
  stripExpressions,
  stripTCalls,
} from './check-i18n.mjs';

describe('stripExpressions', () => {
  it('keeps literal text and drops Svelte expressions', () => {
    expect(stripExpressions("Download {name}")).toBe('Download ');
    expect(stripExpressions("{t('x', { count: 2 })}")).toBe('');
    expect(stripExpressions("a {`x ${y}`} b")).toBe('a  b');
  });
});

describe('stripTCalls', () => {
  it('removes t() arguments but keeps the rest of the expression', () => {
    expect(stripTCalls("open ? t('a') : t('b')").trim()).toBe('open ?  :');
    expect(stripTCalls("t('x', { count: 2 }) + name")).toBe(' + name');
    expect(stripTCalls('obj.t(x)')).toBe('obj.t(x)');
    expect(stripTCalls('it(x)')).toBe('it(x)');
  });
});

describe('hardcodedCopy', () => {
  it('fails on a deliberately hardcoded string', () => {
    const found = hardcodedCopy('<div class="x">Hello there</div>');
    expect(found).toHaveLength(1);
    expect(found[0].text).toBe('Hello there');
  });

  it('passes a swept template where every string comes from the bundle', () => {
    expect(hardcodedCopy('<div>{t(\'x\')}</div>')).toEqual([]);
    expect(hardcodedCopy('<div title={t(\'x\')}>{t(\'y\')}</div>')).toEqual([]);
  });

  it('fails on a user-facing attribute that mixes literal text with an expression', () => {
    const found = hardcodedCopy('<button aria-label="Download {name}">{t(\'x\')}</button>');
    expect(found).toHaveLength(1);
    expect(found[0].kind).toBe('attribute aria-label');
  });

  it('allows brands, acronyms and key glyphs', () => {
    expect(hardcodedCopy('<span>Alloy</span><span>OIDC</span><kbd>↵</kbd><span>A</span>')).toEqual([]);
  });

  it('ignores script and style blocks but keeps line numbers true', () => {
    const source = '<script>\nconst x = "hardcoded in a script";\n</script>\n<p>Visible</p>';
    const found = hardcodedCopy(source);
    expect(found).toHaveLength(1);
    expect(found[0].text).toBe('Visible');
    expect(found[0].line).toBe(4);
  });

  it('fails on prose literals inside a user-facing attribute expression', () => {
    const found = hardcodedCopy(
      "<button aria-label={open ? 'Hide emoji picker' : 'Show emoji picker'}>{t('x')}</button>"
    );
    expect(found).toHaveLength(2);
    expect(found.map((p) => p.kind)).toEqual(['attribute aria-label', 'attribute aria-label']);
    expect(found.map((p) => p.text)).toEqual(['Hide emoji picker', 'Show emoji picker']);
  });

  it('passes attribute expressions whose prose comes from t()', () => {
    expect(hardcodedCopy("<button aria-label={open ? t('a') : t('b')}></button>")).toEqual([]);
    expect(hardcodedCopy("<input placeholder={t('x', { kind })} />")).toEqual([]);
  });

  it('fails on a prose template literal interpolated into a text node', () => {
    const found = hardcodedCopy('<span>{`${members} member(s)`}</span>');
    expect(found).toHaveLength(1);
    expect(found[0].kind).toBe('text interpolation');
    expect(found[0].text).toBe('member(s)');
  });

  it('flags menu copy built inline in a template expression', () => {
    const found = hardcodedCopy(
      "<button use:contextmenu={() => [{ id: 'back', label: 'Back to rooms' }]}>{t('x')}</button>"
    );
    expect(found).toHaveLength(1);
    expect(found[0].kind).toBe('template label');
    expect(found[0].text).toBe('Back to rooms');
  });
});

describe('scriptCopy', () => {
  it('fails on copy keys rendered from a script, including ternary branches', () => {
    const source = [
      '<script>',
      '  const copy = $derived({',
      "    detail: 'You can keep using Alloy',",
      "    action: busy ? 'Retry' : 'Restart',",
      '  });',
      '</script>',
      '<p>{copy.detail}</p>',
    ].join('\n');
    const found = scriptCopy(source);
    expect(found.map((p) => [p.kind, p.text, p.line])).toEqual([
      ['script detail', 'You can keep using Alloy', 3],
      ['script action', 'Retry', 4],
      ['script action', 'Restart', 4],
    ]);
  });

  it('passes script copy routed through t()', () => {
    expect(scriptCopy("<script>\nconst m = { label: t('message-row-reply') };\n</script>")).toEqual([]);
  });
});

describe('moduleCopy', () => {
  it('fails on English labels in a .ts menu module', () => {
    const source = [
      'const choices = [',
      "  { value: 'all', label: 'All messages' },",
      "  { value: 'none', label: `Mute ${kind}` },",
      '];',
    ].join('\n');
    expect(moduleCopy(source).map((p) => [p.kind, p.text])).toEqual([
      ['module label', 'All messages'],
      ['module label', 'Mute'],
    ]);
  });

  it('does not chase protocol identifiers in action/reason values', () => {
    const source = [
      "client.invoke({ action: 'GetOwnProfile', params: {} });",
      "this.failure = { reason: 'launch_failed', message: t('x') };",
    ].join('\n');
    expect(moduleCopy(source)).toEqual([]);
  });

  it('fails on non-object return copy inside a copy-key object (copyKeyProblems)', () => {
    const found = copyKeyProblems("const x = { confirm: 'Remove', message: (n) => `${n} will go.` };");
    expect(found.map((p) => p.text)).toEqual(['Remove', 'will go.']);
  });
});

describe('moduleExcluded', () => {
  it('skips tests, ipc, generated, mock, i18n and platform modules with a reason', () => {
    for (const path of [
      'src/lib/components/shell/roomMenus.test.ts',
      'src/lib/ipc/client.ts',
      'src/lib/generated/protocol.ts',
      'src/lib/mock/data.ts',
      'src/lib/i18n/core.ts',
      'src/lib/platform/contextmenu.ts',
    ]) {
      expect(moduleExcluded(resolve(process.cwd(), path)), path).toBeTruthy();
    }
    expect(moduleExcluded(resolve(process.cwd(), 'src/lib/components/shell/roomMenus.ts'))).toBeNull();
    expect(moduleExcluded(resolve(process.cwd(), 'src/lib/content/mute.ts'))).toBeNull();
  });
});

describe('messageIds', () => {
  it('reads ids and ignores indented attribute/selector lines', () => {
    const source = [
      'alpha-one = Hello',
      'beta-two = { $count ->',
      '    [one] { $count } thing',
      '   *[other] { $count } things',
      '  }',
      'gamma = Value',
    ].join('\n');
    expect([...messageIds(source)]).toEqual(['alpha-one', 'beta-two', 'gamma']);
  });
});
