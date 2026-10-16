<script lang="ts">
  import { onMount } from "svelte";

  let { visible = $bindable(true) }: { visible?: boolean } = $props();

  let fps = $state(60);
  let p50 = $state(16.6);
  let p95 = $state(16.6);
  let minFps = $state(60);
  let droppedFrames = $state(0);

  const frameTimes: number[] = [];
  const MAX_SAMPLES = 120;
  let lastTime = 0;
  let animId: number;

  function loop(now: number) {
    if (lastTime > 0) {
      const delta = now - lastTime;
      frameTimes.push(delta);
      if (frameTimes.length > MAX_SAMPLES) {
        frameTimes.shift();
      }

      if (delta > 20.0) {
        droppedFrames += 1;
      }

      if (frameTimes.length > 10) {
        const sorted = [...frameTimes].sort((a, b) => a - b);
        const currentP50 = sorted[Math.floor(sorted.length * 0.5)];
        const currentP95 = sorted[Math.floor(sorted.length * 0.95)];
        const avg = frameTimes.reduce((acc, v) => acc + v, 0) / frameTimes.length;

        p50 = Math.round(currentP50 * 10) / 10;
        p95 = Math.round(currentP95 * 10) / 10;
        fps = Math.round(1000 / avg);
        if (fps < minFps && frameTimes.length >= MAX_SAMPLES) {
          minFps = fps;
        }
      }
    }
    lastTime = now;
    animId = requestAnimationFrame(loop);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "F2" || (e.ctrlKey && e.key === "h")) {
      e.preventDefault();
      visible = !visible;
    }
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
    animId = requestAnimationFrame(loop);
    return () => {
      window.removeEventListener("keydown", handleKeydown);
      cancelAnimationFrame(animId);
    };
  });
</script>

{#if visible}
  <div class="hud-panel" data-testid="fps-hud">
    <div class="hud-header">
      <span class="hud-dot" class:healthy={fps >= 55} class:warning={fps < 55 && fps >= 45} class:danger={fps < 45}></span>
      <span class="hud-title">ALLOY PERF HUD</span>
      <button class="hud-close" onclick={() => visible = false} title="Close HUD (F2 to toggle)">×</button>
    </div>
    <div class="hud-metrics">
      <div class="metric">
        <span class="label">FPS</span>
        <span class="value" class:good={fps >= 58} class:warn={fps < 58}>{fps}</span>
      </div>
      <div class="metric">
        <span class="label">p50</span>
        <span class="value">{p50} ms</span>
      </div>
      <div class="metric">
        <span class="label">p95</span>
        <span class="value" class:warn={p95 > 25}>{p95} ms</span>
      </div>
      <div class="metric">
        <span class="label">Drops (>20ms)</span>
        <span class="value">{droppedFrames}</span>
      </div>
    </div>
    <div class="hud-hint">Press <strong>F2</strong> or <strong>Ctrl+H</strong> to toggle</div>
  </div>
{/if}

<style>
  .hud-panel {
    position: fixed;
    top: var(--alloy-space-3);
    right: var(--alloy-space-3);
    z-index: 9999;
    background: var(--alloy-bg-overlay);
    border: 1px solid var(--alloy-border-default);
    border-radius: var(--alloy-radius-md);
    padding: var(--alloy-space-3);
    box-shadow: var(--alloy-shadow-md);
    color: var(--alloy-text-primary);
    font-family: var(--alloy-font-mono);
    font-size: 11px;
    width: 220px;
    backdrop-filter: blur(8px);
    user-select: none;
    pointer-events: auto;
  }

  .hud-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--alloy-space-2);
    border-bottom: 1px solid var(--alloy-border-subtle);
    padding-bottom: var(--alloy-space-1);
  }

  .hud-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--alloy-radius-full);
    display: inline-block;
  }

  .hud-dot.healthy {
    background-color: var(--alloy-online);
    box-shadow: 0 0 6px var(--alloy-online);
  }

  .hud-dot.warning {
    background-color: #f59e0b;
  }

  .hud-dot.danger {
    background-color: var(--alloy-unread-badge);
  }

  .hud-title {
    font-weight: 700;
    letter-spacing: 0.5px;
    color: var(--alloy-text-secondary);
  }

  .hud-close {
    background: transparent;
    border: none;
    color: var(--alloy-text-muted);
    font-size: 16px;
    cursor: pointer;
    line-height: 1;
    padding: 0 var(--alloy-space-1);
  }

  .hud-close:hover {
    color: var(--alloy-text-primary);
  }

  .hud-metrics {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--alloy-space-2);
  }

  .metric {
    display: flex;
    flex-direction: column;
  }

  .label {
    font-size: 9px;
    text-transform: uppercase;
    color: var(--alloy-text-muted);
  }

  .value {
    font-size: 13px;
    font-weight: 600;
    color: var(--alloy-text-primary);
  }

  .value.good {
    color: var(--alloy-online);
  }

  .value.warn {
    color: #f59e0b;
  }

  .hud-hint {
    margin-top: var(--alloy-space-2);
    font-size: 9px;
    color: var(--alloy-text-muted);
    text-align: center;
    border-top: 1px solid var(--alloy-border-subtle);
    padding-top: var(--alloy-space-1);
  }
</style>
