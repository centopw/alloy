<script lang="ts">
  import { Button, TextField } from '$lib/components/common';
  import type { CapabilitiesDto, HistoryEnrolmentOutcome } from '$lib/generated/index.js';
  import { errorMessage } from '$lib/stores/errors';

  import { t } from '$lib/i18n';

  interface Props {
    /** What the account allows; the PIN is offered only while a backup exists and none is enrolled. */
    capabilities: CapabilitiesDto;
    /** True while another account command is in flight. */
    busy: boolean;
    /** Hands the PIN to the engine once; the component never stores it. */
    onenrol: (pin: string) => Promise<HistoryEnrolmentOutcome>;
  }

  let { capabilities, busy, onenrol }: Props = $props();

  const id = $props.id();
  let pin = $state('');
  let submitting = $state(false);
  let outcome = $state<HistoryEnrolmentOutcome | null>(null);
  let error = $state<string | null>(null);

  const enrolled = $derived(outcome === 'enrolled' || capabilities.history.enrolled);
  const valid = $derived(/^\d{6}$/.test(pin));

  /** A rejected PIN is the one outcome that carries a number; the rest are bare tags. */
  const attemptsRemaining = $derived(
    outcome !== null && typeof outcome === 'object'
      ? outcome.pin_rejected.attempts_remaining
      : null
  );

  /** The line this account's archive state deserves, if any. */
  const message = $derived.by(() => {
    if (error) return error;
    if (enrolled) return t('history-pin-enrolled');
    if (attemptsRemaining !== null) return t('history-pin-wrong', { attempts: attemptsRemaining });
    switch (outcome) {
      case 'locked_out':
        return t('history-pin-locked');
      case 'pin_backup_unavailable':
        return t('history-pin-no-backup');
      case 'not_supported':
        return t('history-pin-unavailable');
      default:
        return null;
    }
  });

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!valid || busy || submitting || enrolled) return;
    submitting = true;
    error = null;
    outcome = null;
    try {
      outcome = await onenrol(pin);
      // A rejected PIN is discarded rather than left in the field; the next attempt starts empty.
      pin = '';
    } catch (e) {
      error = errorMessage(e);
    } finally {
      submitting = false;
    }
  }
</script>

{#if capabilities.history.pin_backup}
  <div class="history-pin">
    <div class="history-pin-text">
      <h3 class="history-pin-title">{t('history-pin-title')}</h3>
      {#if !enrolled}
        <p class="muted">{t('history-pin-hint')}</p>
      {/if}
      {#if message}
        <p class="note" class:error={error !== null} role={error ? 'alert' : 'status'}>{message}</p>
      {/if}
    </div>
    {#if !enrolled}
      <form class="history-pin-form" onsubmit={submit}>
        <label class="history-pin-label" for="{id}-pin">{t('history-pin-label')}</label>
        <TextField
          id="{id}-pin"
          value={pin}
          type="password"
          inputmode="numeric"
          autocomplete="off"
          maxlength={6}
          pattern="[0-9]{6}"
          spellcheck={false}
          oninput={(e) => {
            pin = e.currentTarget.value.replace(/\D/g, '').slice(0, 6);
            e.currentTarget.value = pin;
          }}
        />
        <Button type="submit" disabled={!valid || busy || submitting} loading={submitting}>
          {t('history-pin-submit')}
        </Button>
      </form>
    {/if}
  </div>
{/if}

<style>
  .history-pin {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-4);
    padding: var(--alloy-spacing-scale-3) 0;
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .history-pin-text {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }
  .history-pin-title {
    margin: 0;
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }
  .muted {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-muted);
  }
  .note {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-muted);
  }
  .note.error {
    color: var(--alloy-color-status-error);
  }
  .history-pin-form {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    flex-shrink: 0;
  }
  .history-pin-label {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  .history-pin-form :global(.text-field) {
    width: 9rem;
    letter-spacing: 0.25em;
    font-variant-numeric: tabular-nums;
  }
  @media (max-width: 899px) {
    .history-pin {
      flex-direction: column;
      align-items: stretch;
    }
    .history-pin-form {
      flex-wrap: wrap;
    }
  }
</style>
