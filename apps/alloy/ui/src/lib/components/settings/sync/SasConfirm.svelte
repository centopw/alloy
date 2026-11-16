<script lang="ts">
  import { Button } from '$lib/components/common';

  import { t } from '$lib/i18n';

  interface Props {
    /** Six digits from the engine. Anything else is ignored by the display. */
    sas: string;
    peerLabel?: string | null;
    onmatch: () => void;
    onnomatch: () => void;
  }

  let { sas, peerLabel = null, onmatch, onnomatch }: Props = $props();

  const digits = $derived(sas.replace(/\D/g, '').slice(0, 6));
  const first = $derived(digits.slice(0, 3));
  const second = $derived(digits.slice(3, 6));
</script>

<div class="sas">
  <h4 class="title">{t('sync-sas-title')}</h4>
  {#if peerLabel}
    <p class="peer">{t('sync-sas-with', { device: peerLabel })}</p>
  {/if}
  <p class="digits" aria-label={t('sync-sas-label')}>
    <span>{first}</span>
    <span>{second}</span>
  </p>
  <p class="warning">{t('sync-sas-warning')}</p>
  <div class="actions">
    <Button variant="danger" onclick={onnomatch}>{t('sync-sas-no-match')}</Button>
    <Button variant="primary" onclick={onmatch}>{t('sync-sas-match')}</Button>
  </div>
</div>

<style>
  .sas {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-4) 0;
  }

  .title {
    margin: 0;
    font-size: var(--alloy-font-size-title);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }

  .peer {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-secondary);
  }

  .digits {
    display: flex;
    gap: var(--alloy-spacing-scale-6);
    margin: 0;
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-typography-size-xxl);
    font-variant-numeric: tabular-nums;
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-spacing-scale-1);
    color: var(--alloy-color-text-primary);
  }

  .warning {
    margin: 0;
    max-width: 34em;
    font-size: var(--alloy-font-size-preview);
    line-height: var(--alloy-typography-line-height-normal);
    color: var(--alloy-color-status-warning);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-2);
  }
</style>
