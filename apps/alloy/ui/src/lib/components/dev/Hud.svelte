<script lang="ts">
  import { onMount } from "svelte";
  import { themeStore } from "../../theme/theme.svelte";

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

    <!-- Theme Switcher Dev Control -->
    <div class="hud-theme-section">
      <div class="section-heading">THEME & DENSITY</div>
      <div class="theme-row">
        <span class="row-label">Mode</span>
        <div class="btn-group">
          <button
            class="control-btn"
            class:active={themeStore.activeTheme === "alloy-light"}
            onclick={() => themeStore.setTheme("alloy-light")}
          >Light</button>
          <button
            class="control-btn"
            class:active={themeStore.activeTheme === "alloy-dark"}
            onclick={() => themeStore.setTheme("alloy-dark")}
          >Dark</button>
          <button
            class="control-btn"
            class:active={themeStore.activeTheme === "alloy-amoled"}
            onclick={() => themeStore.setTheme("alloy-amoled")}
          >AMOLED</button>
        </div>
      </div>
      <div class="theme-row">
        <span class="row-label">Density</span>
        <div class="btn-group">
          <button
            class="control-btn"
            class:active={themeStore.density === "compact"}
            onclick={() => themeStore.setDensity("compact")}
          >Compact</button>
          <button
            class="control-btn"
            class:active={themeStore.density === "normal"}
            onclick={() => themeStore.setDensity("normal")}
          >Normal</button>
          <button
            class="control-btn"
            class:active={themeStore.density === "cozy"}
            onclick={() => themeStore.setDensity("cozy")}
          >Cozy</button>
        </div>
      </div>
      <div class="theme-row">
        <span class="row-label">Style</span>
        <div class="btn-group">
          <button
            class="control-btn"
            class:active={themeStore.messageStyle === "bubble"}
            onclick={() => themeStore.setMessageStyle("bubble")}
          >Bubble</button>
          <button
            class="control-btn"
            class:active={themeStore.messageStyle === "gutter"}
            onclick={() => themeStore.setMessageStyle("gutter")}
          >Gutter</button>
        </div>
      </div>
    </div>

    <div class="hud-hint">Press <strong>F2</strong> or <strong>Ctrl+H</strong> to toggle</div>
  </div>
{/if}

<style>
  .hud-panel {
    position: fixed;
    top: var(--alloy-spacing-scale-3);
    right: var(--alloy-spacing-scale-3);
    z-index: 9999;
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-md);
    padding: var(--alloy-spacing-scale-3);
    box-shadow: var(--alloy-elevation-md);
    color: var(--alloy-color-text-primary);
    font-family: var(--alloy-typography-font-family-mono);
    font-size: 11px;
    width: 230px;
    backdrop-filter: blur(8px);
    user-select: none;
    pointer-events: auto;
  }

  .hud-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--alloy-spacing-scale-2);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    padding-bottom: var(--alloy-spacing-scale-1);
  }

  .hud-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--alloy-radii-full);
    display: inline-block;
  }

  .hud-dot.healthy {
    background-color: var(--alloy-color-status-online);
    box-shadow: 0 0 6px var(--alloy-color-status-online);
  }

  .hud-dot.warning {
    background-color: var(--alloy-color-status-warning);
  }

  .hud-dot.danger {
    background-color: var(--alloy-color-status-error);
  }

  .hud-title {
    font-weight: 700;
    letter-spacing: 0.5px;
    color: var(--alloy-color-text-secondary);
  }

  .hud-close {
    background: transparent;
    border: none;
    color: var(--alloy-color-text-muted);
    font-size: 16px;
    cursor: pointer;
    line-height: 1;
    padding: 0 var(--alloy-spacing-scale-1);
  }

  .hud-close:hover {
    color: var(--alloy-color-text-primary);
  }

  .hud-metrics {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--alloy-spacing-scale-2);
  }

  .metric {
    display: flex;
    flex-direction: column;
  }

  .label {
    font-size: 9px;
    text-transform: uppercase;
    color: var(--alloy-color-text-muted);
  }

  .value {
    font-size: 13px;
    font-weight: 600;
    color: var(--alloy-color-text-primary);
  }

  .value.good {
    color: var(--alloy-color-status-online);
  }

  .value.warn {
    color: var(--alloy-color-status-warning);
  }

  /* Dev control theme switcher */
  .hud-theme-section {
    margin-top: var(--alloy-spacing-scale-2);
    padding-top: var(--alloy-spacing-scale-2);
    border-top: 1px solid var(--alloy-color-border-subtle);
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }

  .section-heading {
    font-size: 9px;
    font-weight: 700;
    color: var(--alloy-color-text-muted);
    letter-spacing: 0.5px;
  }

  .theme-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-1);
  }

  .row-label {
    font-size: 10px;
    color: var(--alloy-color-text-secondary);
    min-width: 48px;
  }

  .btn-group {
    display: flex;
    gap: 2px;
    flex: 1;
    justify-content: flex-end;
  }

  .control-btn {
    background: var(--alloy-color-surface-panel);
    border: 1px solid var(--alloy-color-border-subtle);
    color: var(--alloy-color-text-secondary);
    border-radius: var(--alloy-radii-sm);
    padding: 2px 5px; /* token-audit: HUD chip inset, developer-only surface */
    font-size: 9px;
    font-family: inherit;
    cursor: pointer;
    transition: all 0.1s ease;
  }

  .control-btn:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }

  .control-btn.active {
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
    border-color: var(--alloy-color-accent-primary);
  }

  .hud-hint {
    margin-top: var(--alloy-spacing-scale-2);
    font-size: 9px;
    color: var(--alloy-color-text-muted);
    text-align: center;
    border-top: 1px solid var(--alloy-color-border-subtle);
    padding-top: var(--alloy-spacing-scale-1);
  }
</style>
