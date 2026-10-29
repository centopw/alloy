<script module lang="ts">
  let activePause: (() => void) | null = null;

  /**
   * Module-level singleton ensuring only one player plays at a time.
   * Starting playback in one player automatically pauses any currently active player.
   */
  export function stopOtherPlayers(pauseFn: () => void): void {
    if (activePause && activePause !== pauseFn) {
      activePause();
    }
    activePause = pauseFn;
  }

  export function clearActivePlayer(pauseFn: () => void): void {
    if (activePause === pauseFn) {
      activePause = null;
    }
  }
</script>

<script lang="ts">
  import { tick, untrack } from 'svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Pause, Play, Download } from '$lib/icons';
  import { prefs, type VoiceSpeed } from '$lib/stores/prefs.svelte';
  import { barIndexAt, downsample, fallbackBars } from '$lib/content/waveform';
  import { fetchMediaFile } from '$lib/content/mediaFetch';

  interface Props {
    uri: string;
    mimeType: string;
    body: string;
    sizeBytes: number;
    durationMs?: number | null;
    waveform?: readonly number[] | null;
    voice?: boolean;
    /** Saves the audio file; omitted where the caller cannot download. */
    ondownload?: (() => void) | null;
  }
  let props: Props = $props();
  let audioEl: HTMLAudioElement | undefined = $state();
  let src = $state<string | null>(null);
  let isPlaying = $state(false);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let currentTime = $state(0);
  let audioDuration = $state(0);
  let isDragging = false;
  let pendingSeekTime: number | null = null;
  let prevUri = '';

  const effectiveDuration = $derived.by(() => {
    if (audioDuration > 0 && Number.isFinite(audioDuration)) {
      return audioDuration;
    }
    if (props.durationMs && props.durationMs > 0) {
      return props.durationMs / 1000;
    }
    return 0;
  });

  const bars = $derived.by(() => {
    if (props.waveform && props.waveform.length > 0) {
      return downsample(props.waveform, 64);
    }
    return fallbackBars(props.uri, 64);
  });

  const activeBarIndex = $derived.by(() => {
    if (effectiveDuration <= 0 || currentTime <= 0) return -1;
    const fraction = Math.min(1, currentTime / effectiveDuration);
    return barIndexAt(fraction, 64);
  });

  // Keep playbackRate in sync with prefs.voiceSpeed
  $effect(() => {
    const speed = prefs.voiceSpeed;
    const rate = speed === '2' ? 2 : speed === '1.5' ? 1.5 : 1;
    if (audioEl) {
      audioEl.playbackRate = rate;
    }
  });

  // Reset playback if uri changes
  $effect(() => {
    const currentUri = props.uri;
    if (prevUri !== '' && currentUri !== prevUri) {
      pause();
      src = null;
      currentTime = 0;
      audioDuration = 0;
      error = null;
      pendingSeekTime = null;
    }
    prevUri = currentUri;
  });

  // Cleanup on unmount
  $effect(() => {
    return () => {
      pause();
    };
  });

  function pause(): void {
    if (audioEl) {
      audioEl.pause();
    }
    isPlaying = false;
    clearActivePlayer(pause);
  }

  async function togglePlay(): Promise<void> {
    if (isPlaying) {
      pause();
      return;
    }
    if (loading) return;

    error = null;

    if (!src) {
      loading = true;
      try {
        const url = await fetchMediaFile(props.uri, props.mimeType);
        src = url;
        await tick();
        if (audioEl) {
          if (pendingSeekTime !== null) {
            audioEl.currentTime = pendingSeekTime;
            pendingSeekTime = null;
          }
          await audioEl.play();
        }
      } catch {
        error = 'Could not load audio';
      } finally {
        loading = false;
      }
    } else if (audioEl) {
      loading = true;
      try {
        if (pendingSeekTime !== null) {
          audioEl.currentTime = pendingSeekTime;
          pendingSeekTime = null;
        }
        await audioEl.play();
      } catch {
        error = 'Could not load audio';
      } finally {
        loading = false;
      }
    }
  }

  function cycleSpeed(): void {
    const current = prefs.voiceSpeed;
    const next: VoiceSpeed = current === '1' ? '1.5' : current === '1.5' ? '2' : '1';
    prefs.set('voiceSpeed', next);
  }

  function formatTime(seconds: number): string {
    if (!Number.isFinite(seconds) || seconds < 0) return '0:00';
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  }

  function seekTo(time: number, updateAudio = true): void {
    currentTime = time;
    if (audioEl && Number.isFinite(audioEl.duration) && audioEl.duration > 0) {
      if (updateAudio) {
        audioEl.currentTime = time;
      }
    } else {
      pendingSeekTime = time;
    }
  }

  function seekFromPointer(e: PointerEvent, target: HTMLElement, commitAudio: boolean): void {
    const rect = target.getBoundingClientRect();
    if (rect.width <= 0) return;
    const fraction = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    seekTo(fraction * effectiveDuration, commitAudio);
  }

  function handlePointerDown(e: PointerEvent): void {
    isDragging = true;
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
    seekFromPointer(e, target, true);
  }

  function handlePointerMove(e: PointerEvent): void {
    if (!isDragging) return;
    seekFromPointer(e, e.currentTarget as HTMLElement, false);
  }

  function handlePointerUp(e: PointerEvent): void {
    if (!isDragging) return;
    isDragging = false;
    seekFromPointer(e, e.currentTarget as HTMLElement, true);
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      // Ignored if capture already released
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    const dur = effectiveDuration;
    if (dur <= 0) return;

    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      seekTo(Math.max(0, currentTime - 5), true);
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      seekTo(Math.min(dur, currentTime + 5), true);
    } else if (e.key === 'Home') {
      e.preventDefault();
      seekTo(0, true);
    } else if (e.key === 'End') {
      e.preventDefault();
      seekTo(dur, true);
    }
  }

  function handleTimeUpdate(): void {
    if (!isDragging && audioEl) {
      currentTime = audioEl.currentTime;
    }
  }

  function handleLoadedMetadata(): void {
    if (audioEl && Number.isFinite(audioEl.duration) && audioEl.duration > 0) {
      audioDuration = audioEl.duration;
    }
    const speed = prefs.voiceSpeed;
    const rate = speed === '2' ? 2 : speed === '1.5' ? 1.5 : 1;
    if (audioEl) {
      audioEl.playbackRate = rate;
    }
  }

  function handlePlay(): void {
    stopOtherPlayers(pause);
    isPlaying = true;
    loading = false;
  }

  function handlePause(): void {
    isPlaying = false;
    clearActivePlayer(pause);
  }

  function handleEnded(): void {
    isPlaying = false;
    currentTime = 0;
    clearActivePlayer(pause);
  }

  function handleAudioError(): void {
    error = 'Could not load audio';
    isPlaying = false;
    loading = false;
  }
</script>

<div class="audio-player">
  <audio
    bind:this={audioEl}
    {src}
    preload="metadata"
    ontimeupdate={handleTimeUpdate}
    onloadedmetadata={handleLoadedMetadata}
    ondurationchange={handleLoadedMetadata}
    onplay={handlePlay}
    onpause={handlePause}
    onended={handleEnded}
    onerror={handleAudioError}
  ></audio>

  {#if !props.voice && props.body}
    <div class="header">
      <span class="file-name" title={props.body}>{props.body}</span>
    </div>
  {/if}

  <div class="controls">
    <button
      type="button"
      class="play-btn"
      aria-label={error ? 'Retry loading audio' : isPlaying ? 'Pause audio' : 'Play audio'}
      data-tooltip={error ? 'Retry' : isPlaying ? 'Pause' : 'Play'}
      aria-busy={loading}
      onclick={togglePlay}
    >
      {#if isPlaying}
        <Icon icon={Pause} size={20} />
      {:else}
        <Icon icon={Play} size={20} />
      {/if}
    </button>

    <div class="body-col">
      <div
        class="waveform-container"
        role="slider"
        tabindex="0"
        aria-label="Audio scrubber"
        aria-valuemin="0"
        aria-valuemax={Math.round(effectiveDuration)}
        aria-valuenow={Math.round(currentTime)}
        aria-valuetext="{formatTime(currentTime)} of {formatTime(effectiveDuration)}"
        onpointerdown={handlePointerDown}
        onpointermove={handlePointerMove}
        onpointerup={handlePointerUp}
        onpointercancel={handlePointerUp}
        onkeydown={handleKeydown}
      >
        <svg
          class="waveform-svg"
          viewBox="0 0 192 32"
          preserveAspectRatio="none"
          aria-hidden="true"
        >
          {#each bars as bar, i}
            <rect
              x={i * 3 + 0.5}
              y={(32 - Math.max(3, Math.round(bar * 28))) / 2}
              width="2"
              height={Math.max(3, Math.round(bar * 28))}
              rx="1"
              class={i <= activeBarIndex ? 'filled' : 'unfilled'}
            />
          {/each}
        </svg>
      </div>

      <div class="meta-row">
        {#if error}
          <button type="button" class="error-notice" onclick={togglePlay} aria-label="Retry loading audio">
            <span class="error-text">{error}</span>
            <span class="retry-hint">Retry</span>
          </button>
        {:else}
          <span class="time-text">
            {formatTime(currentTime)} / {formatTime(effectiveDuration)}
          </span>
        {/if}
      </div>
    </div>

    <button
      type="button"
      class="speed-btn"
      aria-label="Playback speed"
      data-tooltip="Playback speed"
      onclick={cycleSpeed}
    >
      {prefs.voiceSpeed}x
    </button>

    <!-- Audio is a file like any other: the tile carries its own save control (#114). -->
    {#if props.ondownload}
      <button
        type="button"
        class="download-btn"
        aria-label="Download {props.body || 'audio'}"
        title="Download"
        onclick={props.ondownload}
      >
        <Icon icon={Download} size={16} />
      </button>
    {/if}
  </div>
</div>

<style>
  .audio-player {
    display: flex;
    flex-direction: column;
    width: 100%;
    min-width: min(260px, 100%);
    max-width: 360px;
    gap: var(--alloy-spacing-scale-1);
    user-select: none;
    box-sizing: border-box;
  }

  .header {
    display: flex;
    align-items: center;
    overflow: hidden;
  }

  .file-name {
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    width: 100%;
  }

  /* The playing surface follows the bubble it sits in: the accent pair on a received bubble, and the
     inverted bubble pair on the own bubble, where accent-on-accent would be invisible. */
  .play-btn {
    width: 40px;
    height: 40px;
    min-width: 40px;
    min-height: 40px;
    border-radius: var(--alloy-radii-full);
    background: var(--audio-play-bg, var(--alloy-color-accent-primary));
    color: var(--audio-play-fg, var(--alloy-color-text-on-accent));
    border: none;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
    transition: filter var(--alloy-motion-duration-fast) ease, transform var(--alloy-motion-duration-fast) ease;
  }

  .play-btn:hover {
    filter: brightness(1.08);
  }

  .play-btn:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }

  .play-btn[aria-busy='true'] {
    opacity: 0.8;
    animation: alloy-pulse 1s infinite alternate ease-in-out;
  }

  @keyframes alloy-pulse {
    0% {
      transform: scale(0.96);
      opacity: 0.7;
    }
    100% {
      transform: scale(1.04);
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .play-btn[aria-busy='true'] {
      animation: none;
    }
    .play-btn {
      transition: none;
    }
  }

  .body-col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .waveform-container {
    width: 100%;
    height: 32px;
    display: flex;
    align-items: center;
    cursor: pointer;
    touch-action: none;
    border-radius: var(--alloy-radii-sm);
  }

  .waveform-container:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }

  .waveform-svg {
    width: 100%;
    height: 28px;
    display: block;
    overflow: visible;
  }

  .waveform-svg rect.unfilled {
    fill: color-mix(in srgb, currentColor 22%, transparent);
  }

  .waveform-svg rect.filled {
    fill: var(--audio-wave-played, var(--alloy-color-accent-primary));
  }

  .meta-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 16px;
    line-height: 1;
  }

  .time-text {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .error-notice {
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-xs);
    text-align: left;
  }

  .error-text {
    font-weight: var(--alloy-typography-weight-medium);
  }

  .retry-hint {
    text-decoration: underline;
    opacity: 0.8;
  }

  /* A quiet chip instead of a filled plate: it reads on the grey received bubble and on the own
     blue, where an accent pill disappeared into the surface. */
  .speed-btn {
    flex-shrink: 0;
    border-radius: var(--alloy-radii-full);
    background: color-mix(in srgb, currentColor 8%, transparent);
    border: 1px solid color-mix(in srgb, currentColor 16%, transparent);
    color: inherit;
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    cursor: pointer;
    line-height: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 28px;
    min-width: 40px;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    transition: background var(--alloy-motion-duration-fast) ease, border-color var(--alloy-motion-duration-fast) ease;
  }

  .speed-btn:hover {
    background: color-mix(in srgb, currentColor 14%, transparent);
    border-color: color-mix(in srgb, currentColor 24%, transparent);
  }

  .speed-btn:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }

  /* Same 28px affordance and radius as the file tile's save control, so both tiles read alike. */
  .download-btn {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }

  .download-btn:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }

  .download-btn:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }

  @media (max-width: 899px) {
    .speed-btn {
      min-height: 44px;
      min-width: 44px;
      padding: 6px var(--alloy-spacing-scale-3); /* token-audit: player inset, tuned to the 34px row */
    }

    .waveform-container {
      min-height: 44px;
    }

    .error-notice {
      min-height: 44px;
    }
  }
</style>
