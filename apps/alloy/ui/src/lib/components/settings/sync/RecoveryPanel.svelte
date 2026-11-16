<script lang="ts">
  import { Button, TextArea } from '$lib/components/common';
  import { copyText } from '$lib/platform/clipboard';
  import { parseRecoveryWords } from './pairing';
  import type { RecoveryView } from './types';

  import { t } from '$lib/i18n';

  interface Props {
    recovery: RecoveryView;
    oncreate: () => void;
    onverify: (words: string[]) => void;
    onwritten: () => void;
  }

  let { recovery, oncreate, onverify, onwritten }: Props = $props();

  const WORD_COUNT = 24;

  let verifyOpen = $state(false);
  let phrase = $state('');
  let submitted = $state(false);
  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  const typedWords = $derived(parseRecoveryWords(phrase));
  const wordsValid = $derived(typedWords.length === WORD_COUNT);

  async function copyPhrase() {
    if (!recovery.words) return;
    try {
      await copyText(recovery.words.join(' '));
      copied = true;
      if (copiedTimer !== undefined) clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 2000);
    } catch {
      copied = false;
    }
  }

  function submitVerify(event: SubmitEvent) {
    event.preventDefault();
    submitted = true;
    if (!wordsValid) return;
    onverify(typedWords);
    phrase = '';
    submitted = false;
  }
</script>

<div class="recovery">
  <p class="warning">{t('sync-recovery-warning')}</p>
  {#if recovery.error}
    <p class="error" role="alert">{recovery.error}</p>
  {/if}

  {#if recovery.words}
    <div class="reveal">
      <h4 class="reveal-title">{t('sync-recovery-created-title')}</h4>
      <p class="hint">{t('sync-recovery-created-hint')}</p>
      <ol class="words">
        {#each recovery.words as word, index (index)}
          <li><span class="number">{index + 1}</span><span class="word">{word}</span></li>
        {/each}
      </ol>
      <div class="actions">
        <Button variant="secondary" size="sm" onclick={() => void copyPhrase()}>
          {copied ? t('sync-recovery-copied') : t('sync-recovery-copy')}
        </Button>
        <Button variant="primary" size="sm" onclick={onwritten}>
          {t('sync-recovery-written')}
        </Button>
      </div>
    </div>
  {:else}
    <div class="state">
      {#if recovery.hasPhrase}
        <p class="status" role="status">{t('sync-recovery-has-phrase')}</p>
      {:else}
        <Button variant="secondary" size="sm" onclick={oncreate}>
          {t('sync-recovery-create')}
        </Button>
      {/if}

      {#if recovery.verified}
        <p class="status" role="status">{t('sync-recovery-verified')}</p>
      {/if}

      {#if verifyOpen}
        <form class="verify" onsubmit={submitVerify}>
          <label class="field">
            <span class="field-label">{t('sync-recovery-verify')}</span>
            <TextArea
              bind:value={phrase}
              rows={4}
              placeholder={t('sync-recovery-verify-placeholder')}
              invalid={submitted && !wordsValid}
            />
          </label>
          {#if submitted && !wordsValid}
            <p class="error" role="alert">{t('sync-recovery-verify-invalid')}</p>
          {/if}
          <div class="actions">
            <Button type="submit" variant="secondary" size="sm">
              {t('sync-recovery-verify-submit')}
            </Button>
          </div>
        </form>
      {:else}
        <Button variant="ghost" size="sm" onclick={() => (verifyOpen = true)}>
          {t('sync-recovery-verify')}
        </Button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .recovery {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
    padding: var(--alloy-spacing-scale-3) 0;
  }

  .warning {
    margin: 0;
    max-width: 40em;
    font-size: var(--alloy-font-size-preview);
    line-height: var(--alloy-typography-line-height-normal);
    color: var(--alloy-color-status-warning);
  }

  .reveal,
  .state,
  .verify {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-3);
  }

  .reveal-title {
    margin: 0;
    font-size: var(--alloy-font-size-title);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }

  .hint,
  .status,
  .error {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
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

  .words {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(9rem, 1fr));
    gap: var(--alloy-spacing-scale-2);
    margin: 0;
    padding: var(--alloy-spacing-scale-3);
    width: 100%;
    list-style: none;
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-subtle);
  }

  .words li {
    display: flex;
    align-items: baseline;
    gap: var(--alloy-spacing-scale-2);
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-primary);
  }

  .number {
    min-width: 1.5rem;
    color: var(--alloy-color-text-muted);
    text-align: end;
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

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-2);
  }
</style>
