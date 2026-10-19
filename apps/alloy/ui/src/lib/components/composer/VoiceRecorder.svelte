<script lang="ts">
  import { onDestroy } from 'svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Lock, Mic, Send, Trash2 } from '$lib/icons';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { ClipTooShort, VoiceRecorder, formatDuration, type VoiceClip } from '$lib/platform/voiceRecorder';

  interface Props {
    /** Receives the finished clip. */
    onsend: (clip: VoiceClip) => void | Promise<void>;
    onerror: (message: string) => void;
    disabled?: boolean;
    /** True while the recorder bar is shown, so the parent can hide its text field. */
    recording?: boolean;
  }
  let { onsend, onerror, disabled = false, recording = $bindable(false) }: Props = $props();

  /** Slide this far left to cancel and this far up to lock, on a phone. */
  const CANCEL_PX = 80;
  const LOCK_PX = 60;
  const POLL_MS = 80;

  const recorder = new VoiceRecorder();
  const phone = $derived(viewport.isPhone);

  let elapsedMs = $state(0);
  let levels = $state<number[]>([]);
  let locked = $state(false);
  let dragX = $state(0);
  let dragY = $state(0);
  /** A phone press is in progress; moves and the release are read from the window. */
  let holding = $state(false);
  let originX = 0;
  let originY = 0;
  let starting: Promise<boolean> | null = null;

  const cancelling = $derived(holding && dragX < -CANCEL_PX);

  // The recorder keeps plain fields, so the bar reads them on a short timer while it is open.
  $effect(() => {
    if (!recording) return;
    const timer = setInterval(() => {
      elapsedMs = recorder.elapsedMs;
      levels = [...recorder.levels];
    }, POLL_MS);
    return () => clearInterval(timer);
  });

  /** Starts the recorder. Resolves false when the microphone could not be opened (the error was reported). */
  function begin(): Promise<boolean> {
    recording = true;
    elapsedMs = 0;
    levels = [];
    locked = false;
    return recorder
      .start((clip) => {
        // The 15 minute limit ends the clip by itself.
        recording = false;
        holding = false;
        void onsend(clip);
      })
      .then(() => true)
      .catch((err: unknown) => {
        recording = false;
        holding = false;
        onerror(err instanceof Error ? err.message : 'Recording failed');
        return false;
      });
  }

  async function finish(): Promise<void> {
    if (!recording) return;
    try {
      const clip = await recorder.stop();
      recording = false;
      await onsend(clip);
    } catch (err) {
      recording = false;
      // A press that was too short to be a message is an accidental tap.
      if (!(err instanceof ClipTooShort)) onerror(err instanceof Error ? err.message : 'Could not save the recording');
    }
  }

  function discard(): void {
    recorder.cancel();
    recording = false;
    holding = false;
    locked = false;
  }

  function onMicPointerDown(e: PointerEvent) {
    if (disabled || recording || e.button !== 0) return;
    if (phone) {
      // Hold to record: the window follows the finger, so the button may disappear under it.
      e.preventDefault();
      holding = true;
      dragX = 0;
      dragY = 0;
      originX = e.clientX;
      originY = e.clientY;
      starting = begin();
    }
  }

  function onMicClick() {
    // Desktop: a click starts, and the bar's Send button finishes.
    if (!phone && !disabled && !recording) starting = begin();
  }

  function onWindowMove(e: PointerEvent) {
    if (!holding) return;
    dragX = Math.min(0, e.clientX - originX);
    dragY = Math.min(0, e.clientY - originY);
    if (!locked && dragY < -LOCK_PX && dragX > -CANCEL_PX) {
      locked = true;
      holding = false;
      navigator.vibrate?.(10);
    }
  }

  async function onWindowUp() {
    if (!holding) return;
    holding = false;
    const started = await starting;
    if (!started || !recording) return;
    if (dragX < -CANCEL_PX) {
      navigator.vibrate?.(10);
      discard();
    } else {
      await finish();
    }
  }

  function onWindowCancel() {
    if (holding) {
      holding = false;
      discard();
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if (recording && e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      discard();
    }
  }

  onDestroy(() => recorder.destroy());
</script>

<svelte:window onkeydown={onKeyDown} onpointermove={onWindowMove} onpointerup={onWindowUp} onpointercancel={onWindowCancel} />

{#if recording}
  <div class="bar" role="group" aria-label="Recording a voice message">
    <span class="dot" class:locked aria-hidden="true"></span>
    <span class="time" aria-live="off">{formatDuration(elapsedMs)}</span>
    {#if holding}
      <span class="hint" class:warn={cancelling} role="status">
        {cancelling ? 'Release to cancel' : 'Slide left to cancel'}
      </span>
    {:else}
      <span class="wave" aria-hidden="true">
        {#each levels as level, i (i)}
          <i style:height="{Math.max(3, Math.round(level * 28))}px"></i>
        {/each}
      </span>
    {/if}
    {#if !holding}
      <button type="button" class="act danger" aria-label="Delete recording" data-tooltip="Delete" onclick={discard}>
        <Icon icon={Trash2} size={20} />
      </button>
      <button type="button" class="act send" aria-label="Send voice message" data-tooltip="Send" onclick={() => void finish()}>
        <Icon icon={Send} size={20} />
      </button>
    {/if}
  </div>
{/if}

{#if !recording || holding}
  {#if holding && !cancelling}
    <span class="lock-hint" aria-hidden="true"><Icon icon={Lock} size={16} /></span>
  {/if}
  <button
    type="button"
    class="mic"
    class:held={holding}
    style:transform={holding ? `translate(${dragX}px, ${dragY}px) scale(1.25)` : undefined}
    {disabled}
    aria-label="Record voice message"
    data-tooltip={phone ? 'Hold to record' : 'Record voice message'}
    onpointerdown={onMicPointerDown}
    onclick={onMicClick}
  >
    <Icon icon={Mic} size={20} />
  </button>
{/if}

<style>
  .mic {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
    touch-action: none;
    user-select: none;
    -webkit-touch-callout: none;
  }
  .mic:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .mic.held {
    position: relative;
    z-index: 2;
    background: var(--alloy-color-accent-fill);
    color: var(--alloy-color-text-on-accent);
  }
  @media (hover: hover) {
    .mic:hover:not(:disabled):not(.held) {
      background: var(--alloy-color-surface-hover);
      color: var(--alloy-color-text-primary);
    }
  }
  .lock-hint {
    position: absolute;
    right: var(--alloy-spacing-scale-4);
    bottom: 100%;
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    margin-bottom: var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-elevated);
    box-shadow: var(--alloy-elevation-md);
    color: var(--alloy-color-text-secondary);
  }
  .bar {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    min-width: 0;
    min-height: 36px;
    padding: 0 var(--alloy-spacing-scale-2);
  }
  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-status-error);
    animation: alloy-rec-pulse 1.2s ease-in-out infinite;
  }
  .dot.locked {
    animation: none;
  }
  .time {
    flex: none;
    min-width: 3ch;
    font-variant-numeric: tabular-nums;
    color: var(--alloy-color-text-primary);
  }
  .wave {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: flex-end;
    gap: 2px;
    min-width: 0;
    height: 28px;
    overflow: hidden;
  }
  .wave i {
    flex: none;
    width: 3px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
  }
  .hint {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-sm);
  }
  .hint.warn {
    color: var(--alloy-color-status-error);
  }
  .act {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .act.danger {
    color: var(--alloy-color-status-error);
  }
  .act.send {
    background: var(--alloy-color-accent-fill);
    color: var(--alloy-color-text-on-accent);
  }
  @media (max-width: 899px) {
    .mic,
    .act {
      width: 44px;
      height: 44px;
    }
  }
  @keyframes alloy-rec-pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.35;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .dot {
      animation: none;
    }
  }
</style>
