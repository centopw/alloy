import { describe, expect, it } from 'vitest';
import { hardcodedCopy, messageIds, stripExpressions } from './check-i18n.mjs';

describe('stripExpressions', () => {
  it('keeps literal text and drops Svelte expressions', () => {
    expect(stripExpressions("Download {name}")).toBe('Download ');
    expect(stripExpressions("{t('x', { count: 2 })}")).toBe('');
    expect(stripExpressions("a {`x ${y}`} b")).toBe('a  b');
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
