import type { HistoryLimit } from '$lib/generated';
import { t } from '$lib/i18n';

/**
 * The one line the timeline shows where history runs out.
 *
 * `start` is a real answer — the conversation begins there — so it renders nothing. Every other
 * reason names why older messages cannot be read on this device instead of pretending the
 * conversation began here.
 */
export function historyLimitNotice(limit: HistoryLimit | null | undefined): string | null {
  if (!limit || limit === 'start') return null;
  if (typeof limit === 'object') return t('history-limit-pin-rejected');
  switch (limit) {
    case 'no_backup':
      return t('history-limit-no-backup');
    case 'not_enrolled':
      return t('history-limit-not-enrolled');
    case 'locked_out':
      return t('history-limit-locked');
    case 'not_in_archive':
      return t('history-limit-not-in-archive');
    case 'unreadable':
      return t('history-limit-unreadable');
  }
}
