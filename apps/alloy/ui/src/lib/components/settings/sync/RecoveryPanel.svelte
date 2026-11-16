<script lang="ts">
  import { Button, TextArea, TextField } from '$lib/components/common';
  import { copyText } from '$lib/platform/clipboard';
  import { formatSyncTime, groupPin } from './format';
  import { parseRecoveryWords } from './pairing';
  import type { RecoverFromPhraseInput, RecoveryView } from './types';

  import { t } from '$lib/i18n';

  interface Props {
    recovery: RecoveryView;
    /** No configuration on this device: the phrase plus the server is the only way back. */
    fresh?: boolean;
    oncreate: () => void;
    onverify: (words: string[]) => void;
    onwritten: () => void;
    onrecover?: (input: RecoverFromPhraseInput) => void;
  }

  let { recovery, fresh = false, oncreate, onverify, onwritten, onrecover = () => {} }: Props =
    $props();

  const WORD_COUNT = 24;

  let verifyOpen = $state(false);
  let phrase = $state('');
  let submitted = $state(false);
  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  // The recovery form's own fields. The token is write-only: it is handed to the engine once and
  // the field is cleared, because nothing on this page may echo it back.
  let recoverPhrase = $state('');
  let recoverServer = $state('');
  let recoverToken = $state('');
  let recoverSubmitted = $state(false);

  const typedWords = $derived(parseRecoveryWords(phrase));
  const wordsValid = $derived(typedWords.length === WORD_COUNT);
  const recoveredWords = $derived(parseRecoveryWords(recoverPhrase));
  const recoveredWordsValid = $derived(recoveredWords.length === WORD_COUNT);
  const recovering = $derived(recovery.recovering === true);
  const recoveredPin = $derived(recovery.recovered?.pin ? groupPin(recovery.recovered.pin) : null);
  const rosterNames = $derived(
    (recovery.recovered?.roster ?? []).map((device) => device.label || t('sync-unknown')).join(' · ')
  );

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

  function submitRecover(event: SubmitEvent) {
    event.preventDefault();
    recoverSubmitted = true;
    if (!recoveredWordsValid || !recoverServer.trim() || !recoverToken.trim()) return;
    onrecover({
      words: recoveredWords,
      url: recoverServer.trim(),
      token: recoverToken.trim(),
    });
    // Write-only: the value never survives the call, whether the phrase was accepted or not.
    recoverToken = '';
    recoverSubmitted = false;
  }

  /** One user-facing name per store layer; an unknown key is shown as the engine names it. */
  function layerLabel(key: string): string {
    switch (key) {
      case 'settings':
        return t('sync-layer-settings');
      case 'tags':
        return t('sync-layer-tags');
      case 'catalogue':
        return t('sync-layer-catalogue');
      case 'backgrounds':
        return t('sync-layer-backgrounds');
      case 'theme':
        return t('sync-layer-theme');
      case 'messages':
        return t('sync-layer-messages');
      default:
        return key;
    }
  }

  function layerTime(ms: number): string {
    if (ms <= 0) return t('sync-layer-never');
    return formatSyncTime(ms) ?? t('sync-layer-never');
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

    {#if fresh && !recovery.recovered}
      <form class="recover" onsubmit={submitRecover}>
        <h4 class="reveal-title">{t('sync-recovery-recover-title')}</h4>
        <p class="hint">{t('sync-recovery-recover-description')}</p>

        <label class="field">
          <span class="field-label">{t('sync-recovery-phrase-label')}</span>
          <TextArea
            bind:value={recoverPhrase}
            rows={4}
            placeholder={t('sync-recovery-verify-placeholder')}
            invalid={recoverSubmitted && !recoveredWordsValid}
            disabled={recovering}
          />
        </label>
        {#if recoverSubmitted && !recoveredWordsValid}
          <p class="error" role="alert">{t('sync-recovery-verify-invalid')}</p>
        {/if}

        <label class="field">
          <span class="field-label">{t('sync-recovery-server-label')}</span>
          <TextField
            bind:value={recoverServer}
            type="url"
            autocomplete="url"
            placeholder={t('sync-server-url-placeholder')}
            aria-label={t('sync-recovery-server-label')}
            disabled={recovering}
          />
        </label>

        <label class="field">
          <span class="field-label">{t('sync-recovery-token-label')}</span>
          <TextField
            bind:value={recoverToken}
            type="password"
            autocomplete="off"
            placeholder={t('sync-server-token-placeholder')}
            aria-label={t('sync-recovery-token-label')}
            disabled={recovering}
          />
        </label>

        <!-- The honesty rule: there is no unverified connection to opt into. -->
        <p class="hint">{t('sync-recovery-pin-honesty')}</p>

        {#if recoverSubmitted && (!recoverServer.trim() || !recoverToken.trim())}
          <p class="error" role="alert">{t('sync-recovery-fields-required')}</p>
        {/if}

        <div class="actions">
          <Button type="submit" variant="primary" size="sm" loading={recovering} disabled={recovering}>
            {recovering ? t('sync-recovery-recovering') : t('sync-recovery-recover-submit')}
          </Button>
        </div>
      </form>
    {/if}

    {#if recovery.recovered}
      <div class="recovered">
        <p class="status" role="status">{t('sync-recovery-recovered-title')}</p>
        <p class="hint">{t('sync-recovery-roster', { devices: rosterNames })}</p>
        {#if recoveredPin}
          <p class="hint">{t('sync-recovery-pin-recorded', { pin: recoveredPin })}</p>
        {/if}
        <ul class="layers">
          {#each recovery.recovered.layers as layer (layer.key)}
            <li class="layer" class:layer-off={!layer.enabled}>
              <span class="layer-key">{layerLabel(layer.key)}</span>
              <span class="layer-state">
                {layer.enabled ? t('sync-layer-enabled') : t('sync-layer-disabled')}
              </span>
              <span class="layer-time">{layerTime(layer.updatedAtMs)}</span>
            </li>
          {/each}
        </ul>
      </div>
    {/if}
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
  .verify,
  .recover,
  .recovered {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-3);
  }

  .recover {
    width: 100%;
  }

  .layers {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    margin: 0;
    padding: var(--alloy-spacing-scale-3);
    width: 100%;
    list-style: none;
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-subtle);
  }

  .layer {
    display: grid;
    grid-template-columns: minmax(8rem, 1fr) auto auto;
    gap: var(--alloy-spacing-scale-3);
    align-items: baseline;
    font-size: var(--alloy-typography-size-sm);
  }

  .layer-key {
    color: var(--alloy-color-text-primary);
  }

  .layer-state,
  .layer-time {
    color: var(--alloy-color-text-secondary);
  }

  .layer-off .layer-key,
  .layer-off .layer-state {
    color: var(--alloy-color-text-muted);
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
