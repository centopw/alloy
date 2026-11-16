<script lang="ts">
  import { Button, TextArea, TextField } from '$lib/components/common';
  import {
    isCompletePairingCode,
    normalizePairingCode,
    PAIRING_CODE_LENGTH,
  } from './pairing';

  import { t } from '$lib/i18n';

  interface Props {
    onsubmit: (ticket: string, code: string) => void;
    disabled?: boolean;
  }

  let { onsubmit, disabled = false }: Props = $props();

  let ticket = $state('');
  let code = $state('');
  let submitted = $state(false);

  const ticketValid = $derived(ticket.trim().length > 0);
  const codeValid = $derived(isCompletePairingCode(code));
  const canSubmit = $derived(ticketValid && codeValid && !disabled);

  /**
   * Upper-cases as typed and drops pasted spaces and dashes. The DOM value is rewritten in place so
   * the field never shows a character the submitted code does not contain.
   */
  function onCodeInput(event: Event & { currentTarget: HTMLInputElement }) {
    const input = event.currentTarget;
    const normalized = normalizePairingCode(input.value);
    if (input.value !== normalized) input.value = normalized;
    code = normalized;
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    submitted = true;
    if (!canSubmit) return;
    onsubmit(ticket.trim(), normalizePairingCode(code));
  }
</script>

<form class="accept" onsubmit={submit}>
  <label class="field">
    <span class="field-label">{t('sync-accept-ticket')}</span>
    <TextArea
      bind:value={ticket}
      rows={3}
      disabled={disabled}
      placeholder={t('sync-accept-ticket-placeholder')}
      invalid={submitted && !ticketValid}
    />
  </label>

  <label class="field">
    <span class="field-label">{t('sync-accept-code')}</span>
    <TextField
      value={code}
      oninput={onCodeInput}
      disabled={disabled}
      maxlength={PAIRING_CODE_LENGTH}
      autocomplete="off"
      autocapitalize="characters"
      spellcheck={false}
      placeholder={t('sync-accept-code-placeholder')}
      invalid={submitted && !codeValid}
    />
    {#if submitted && !codeValid}
      <span class="error" role="alert">{t('sync-accept-invalid-code')}</span>
    {:else if submitted && !ticketValid}
      <span class="error" role="alert">{t('sync-accept-invalid-ticket')}</span>
    {/if}
  </label>

  <p class="hint">{t('sync-accept-hint')}</p>

  <div class="actions">
    <Button type="submit" variant="primary" disabled={disabled}>{t('sync-accept-submit')}</Button>
  </div>
</form>

<style>
  .accept {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-5);
    padding: var(--alloy-spacing-scale-4) 0;
    width: 100%;
    max-width: 34rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    width: 100%;
  }

  .field-label {
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }

  .hint,
  .error {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
  }

  .hint {
    color: var(--alloy-color-text-muted);
  }

  .error {
    color: var(--alloy-color-status-error);
  }

  .actions {
    display: flex;
    gap: var(--alloy-spacing-scale-2);
  }
</style>
