import { describe, expect, it } from 'vitest';
import { historyLimitNotice } from './historyLimit';

describe('historyLimitNotice', () => {
  it('renders nothing for a real start of conversation', () => {
    expect(historyLimitNotice('start')).toBeNull();
    expect(historyLimitNotice(null)).toBeNull();
    expect(historyLimitNotice(undefined)).toBeNull();
  });

  it('names the reason older history cannot be read, one line per reason', () => {
    expect(historyLimitNotice('no_backup')).toContain('no encrypted backup');
    expect(historyLimitNotice('not_enrolled')).toContain('encrypted backup');
    expect(historyLimitNotice('locked_out')).toContain('locked');
    expect(historyLimitNotice('not_in_archive')).toContain('not in the encrypted backup');
    expect(historyLimitNotice('unreadable')).toContain('could not be read');
  });

  it('renders a rejected PIN as its own line', () => {
    const line = historyLimitNotice({ pin_rejected: { attempts_remaining: 2 } });
    expect(line).toContain('rejected the PIN');
  });
});
