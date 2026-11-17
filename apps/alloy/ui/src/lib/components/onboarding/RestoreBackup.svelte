<script lang="ts">
  import { Button, TextArea } from '../common';
  import { syncErrorMessage } from '../settings/sync/errors';
  import { client } from '$lib/ipc';
  import { expect } from '$lib/stores/errors';
  import type { ConfigImportReport } from '$lib/generated';

  import { t } from '$lib/i18n';

  interface Props {
    onback: () => void;
    /** Restored with no account stored: the network list is the next step. */
    onaddaccount: () => void;
  }

  let { onback, onaddaccount }: Props = $props();

  let bundle = $state('');
  let busy = $state(false);
  let failure = $state<string | null>(null);
  let report = $state<ConfigImportReport | null>(null);

  const ready = $derived(bundle.trim().length > 0 && !busy);

  /** The engine validates every layer before the first write, so a rejection leaves this device as it was. */
  async function restore() {
    const value = bundle.trim();
    if (!value || busy) return;
    busy = true;
    failure = null;
    try {
      const res = expect(
        await client.invoke({
          action: 'ImportConfigBundle',
          params: { bundle_json: value, apply: true },
        }),
        'ConfigBundleImported'
      );
      report = res.payload.report;
    } catch (e) {
      failure = syncErrorMessage(e);
    } finally {
      busy = false;
    }
  }

  function parts(): string[] {
    if (!report) return [];
    const list: string[] = [];
    if (report.settings) list.push(t('sync-import-part-settings'));
    if (report.catalogue) list.push(t('sync-import-part-catalogue'));
    if (report.tags > 0) list.push(t('sync-import-part-tags', { count: report.tags }));
    if (report.backgrounds > 0) {
      list.push(t('sync-import-part-backgrounds', { count: report.backgrounds }));
    }
    if (report.theme) list.push(t('sync-import-part-theme'));
    return list;
  }

  const arrived = $derived(parts());
</script>

<div class="restore">
  <div class="step-head">
    <p class="eyebrow">{t('onboarding-restore-eyebrow')}</p>
    <h3>{t('onboarding-restore-title')}</h3>
    <p class="hint">{t('onboarding-restore-hint')}</p>
  </div>

  {#if report}
    <p class="status" role="status">{t('onboarding-restore-done')}</p>
    {#if arrived.length > 0}
      <p class="hint">{t('onboarding-restore-arrived', { parts: arrived.join(', ') })}</p>
    {/if}
    <div class="actions">
      <Button type="button" onclick={onaddaccount}>{t('onboarding-link-add-account')}</Button>
    </div>
  {:else}
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void restore();
      }}
    >
      <label class="field">
        <span class="field-label">{t('onboarding-restore-label')}</span>
        <TextArea
          bind:value={bundle}
          rows={5}
          disabled={busy}
          placeholder={t('onboarding-restore-placeholder')}
        />
      </label>
      {#if failure}
        <p class="error" role="alert">{failure}</p>
      {/if}
      <div class="actions">
        <Button variant="ghost" type="button" onclick={onback} disabled={busy}>
          {t('common-back')}
        </Button>
        <Button type="submit" disabled={!ready}>
          {busy ? t('onboarding-working') : t('onboarding-restore-submit')}
        </Button>
      </div>
    </form>
  {/if}
</div>

<style>
  .restore {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-4);
  }
  .step-head {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }
  .eyebrow {
    margin: 0;
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-letter-spacing-caps, 0.08em);
    text-transform: uppercase;
    color: var(--alloy-color-accent-primary);
  }
  h3 {
    margin: 0;
    font-size: var(--alloy-typography-size-xl);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: -0.01em;
    color: var(--alloy-color-text-primary);
  }
  form {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
    width: 100%;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  .field-label {
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }
  .hint,
  .status,
  .error {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    line-height: var(--alloy-typography-line-height-normal, 1.45);
  }
  .hint {
    color: var(--alloy-color-text-muted);
  }
  .status {
    color: var(--alloy-color-text-secondary);
  }
  .error {
    color: var(--alloy-color-status-error);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }
</style>
