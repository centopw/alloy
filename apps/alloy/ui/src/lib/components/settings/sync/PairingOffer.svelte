<script lang="ts">
  import { toString as qrToString } from 'qrcode';
  import { Button } from '$lib/components/common';
  import { copyText } from '$lib/platform/clipboard';

  import { t } from '$lib/i18n';

  interface Props {
    /** QR payload string minted by the engine; null until it exists. */
    ticket: string | null;
    code: string | null;
    expiresAtMs: number | null;
  }

  let { ticket, code, expiresAtMs }: Props = $props();

  /** Fixed QR size (13rem): the offer panel is the same box in every state, so nothing jumps. */
  const QR_SIZE = 208;

  let qrSvg = $state<string | null>(null);
  let qrFailed = $state(false);
  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const payload = ticket;
    if (!payload) {
      qrSvg = null;
      qrFailed = false;
      return;
    }
    let live = true;
    qrFailed = false;
    void qrToString(payload, {
      type: 'svg',
      margin: 2,
      width: QR_SIZE,
      errorCorrectionLevel: 'M',
    })
      .then((svg) => {
        if (live) qrSvg = svg;
      })
      .catch(() => {
        if (!live) return;
        qrSvg = null;
        qrFailed = true;
      });
    return () => {
      live = false;
    };
  });

  // The countdown ticks only while an expiry is known.
  let now = $state(Date.now());
  $effect(() => {
    if (expiresAtMs === null) return;
    const timer = setInterval(() => {
      now = Date.now();
    }, 1000);
    return () => clearInterval(timer);
  });
  const secondsLeft = $derived(
    expiresAtMs === null ? null : Math.max(0, Math.ceil((expiresAtMs - now) / 1000))
  );
  /** The engine retires the pairing at the same deadline; until its event lands, offer nothing. */
  const expired = $derived(secondsLeft === 0);

  async function copyCode() {
    if (!code) return;
    try {
      await copyText(code);
      copied = true;
      if (copiedTimer !== undefined) clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 2000);
    } catch {
      copied = false;
    }
  }
</script>

<div class="offer">
  {#if expired}
    <p class="expiry">{t('sync-offer-expired')}</p>
  {:else}
    <div class="qr-frame">
      {#if qrSvg}
        <div class="qr" role="img" aria-label={t('sync-offer-qr-label')}>
          {@html qrSvg}
        </div>
      {:else if qrFailed}
        <p class="qr-failure">{t('sync-offer-qr-failed')}</p>
      {:else}
        <span class="qr-wait" role="img" aria-label={t('sync-offer-qr-label')}></span>
      {/if}
    </div>

    <p class="hint">{t('sync-offer-hint')}</p>

    <div class="code-row">
      <span class="code-label">{t('sync-offer-code')}</span>
      <output class="code">{code ?? ''}</output>
      <Button variant="ghost" size="sm" disabled={!code} onclick={() => void copyCode()}>
        {copied ? t('sync-offer-copied') : t('sync-offer-copy-code')}
      </Button>
    </div>

    {#if secondsLeft !== null}
      <p class="expiry">{t('sync-offer-expires-in', { seconds: secondsLeft })}</p>
    {/if}
  {/if}

  <p class="crosses">{t('sync-pair-crosses')}</p>
</div>

<style>
  .offer {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-4) 0;
  }

  .qr-frame {
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 13rem;
    height: 13rem;
    padding: var(--alloy-spacing-scale-2);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
  }

  .qr {
    display: block;
    width: 100%;
    height: 100%;
    color: var(--alloy-color-text-primary);
  }

  /* qrcode emits its own black-on-white markup; a scanner needs exactly that, so no theming. */
  .qr :global(svg) {
    display: block;
    width: 100%;
    height: 100%;
  }

  .qr-wait {
    width: 100%;
    height: 100%;
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-subtle);
  }

  .qr-failure {
    margin: 0;
    padding: var(--alloy-spacing-scale-4);
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-muted);
    text-align: center;
  }

  .hint,
  .expiry,
  .crosses {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-secondary);
  }

  .crosses {
    color: var(--alloy-color-text-muted);
  }

  .code-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }

  .code-label {
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-muted);
  }

  .code {
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-primary);
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-typography-size-lg);
    letter-spacing: var(--alloy-spacing-scale-1);
  }
</style>
