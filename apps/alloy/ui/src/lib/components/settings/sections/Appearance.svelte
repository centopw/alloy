<script lang="ts">
  import { SegmentedControl } from '$lib/components/common';
  import type { Snippet } from 'svelte';
  import {
    SettingGroup,
    SettingRadioCards,
    SettingSelect,
    SettingToggle,
  } from '$lib/components/settings/controls';
  import { prefs, type FontScale, type MessageLayout, type TimeFormat } from '$lib/stores/prefs.svelte';
  import { themeStore } from '$lib/theme/theme.svelte';
  import type { Density } from '$lib/theme/theme.svelte';

  const dark = $derived(themeStore.activeTheme !== 'alloy-light');
  const amoled = $derived(themeStore.activeTheme === 'alloy-amoled');

  function setDark(on: boolean) {
    themeStore.setTheme(on ? (amoled ? 'alloy-amoled' : 'alloy-dark') : 'alloy-light');
  }
  function setAmoled(on: boolean) {
    themeStore.setTheme(on ? 'alloy-amoled' : 'alloy-dark');
  }

  const densityOptions: { value: Density; label: string }[] = [
    { value: 'compact', label: 'Compact' },
    { value: 'normal', label: 'Normal' },
    { value: 'cozy', label: 'Cozy' },
  ];

  const fontScaleOptions = [90, 100, 110, 120, 130].map((scale) => ({
    value: String(scale),
    label: scale === 100 ? '100% (default)' : `${scale}%`,
  }));

  const timeFormatOptions: { value: TimeFormat; label: string }[] = [
    { value: 'auto', label: 'Automatic' },
    { value: '12h', label: '12-hour' },
    { value: '24h', label: '24-hour' },
  ];

  // Previews are static markup; they are hoisted, so the option list can reference them.
  const layoutOptions: { value: MessageLayout; label: string; description: string; preview: Snippet }[] = [
    { value: 'modern', label: 'Modern', description: 'Flat rows with avatars', preview: modernPreview },
    { value: 'bubbles', label: 'Bubbles', description: 'Messages in chat bubbles', preview: bubblesPreview },
    { value: 'irc', label: 'IRC', description: 'Compact, single line', preview: ircPreview },
  ];

  const themeReason = 'Follows your system setting.';
  const amoledReason = $derived(
    themeStore.followSystem ? themeReason : !dark ? 'Available in dark mode.' : undefined,
  );
</script>

{#snippet modernPreview()}
  <span class="preview modern" aria-hidden="true">
    <span class="pv-avatar">A</span>
    <span class="pv-body">
      <span class="pv-meta">
        <span class="pv-name">Alex</span>
        <span class="pv-time">10:42</span>
      </span>
      <span class="pv-text">Hey you. You're the best!</span>
    </span>
  </span>
{/snippet}

{#snippet bubblesPreview()}
  <span class="preview bubbles" aria-hidden="true">
    <span class="pv-bubble">Hey you. You're the best!</span>
  </span>
{/snippet}

{#snippet ircPreview()}
  <span class="preview irc" aria-hidden="true">
    <span class="pv-time">10:42</span>
    <span class="pv-nick">&lt;Alex&gt;</span>
    <span class="pv-text">Hey you. You're the best!</span>
  </span>
{/snippet}

<div class="section">
  <SettingGroup title="Theme">
    <SettingToggle
      label="Follow system appearance"
      checked={themeStore.followSystem}
      onchange={(v) => themeStore.setFollowSystem(v)}
    />
    <SettingToggle
      label="Dark mode"
      checked={dark}
      disabled={themeStore.followSystem}
      disabledReason={themeStore.followSystem ? themeReason : undefined}
      onchange={setDark}
    />
    <SettingToggle
      label="Pure black background"
      checked={amoled}
      disabled={!dark || themeStore.followSystem}
      disabledReason={amoledReason}
      onchange={setAmoled}
    />
    <div class="row">
      <span class="row-label">Density</span>
      <SegmentedControl
        label="Density"
        options={densityOptions}
        value={themeStore.density}
        onchange={(v) => themeStore.setDensity(v as Density)}
      />
    </div>
  </SettingGroup>

  <SettingGroup title="Messages">
    <div class="padded">
      <SettingRadioCards
        legend="Message layout"
        value={prefs.messageLayout}
        options={layoutOptions}
        onchange={(v) => prefs.set('messageLayout', v as MessageLayout)}
      />
    </div>
    <SettingToggle
      label="Enable big emoji in chat"
      checked={prefs.bigEmoji}
      onchange={(v) => prefs.set('bigEmoji', v)}
    />
    <SettingToggle
      label="Show who has read your messages"
      checked={prefs.showReadReceipts}
      onchange={(v) => prefs.set('showReadReceipts', v)}
    />
  </SettingGroup>

  <SettingGroup title="Text and time">
    <SettingSelect
      label="Font size"
      value={String(prefs.fontScale)}
      options={fontScaleOptions}
      onchange={(v) => prefs.set('fontScale', Number(v) as FontScale)}
    />
    <SettingSelect
      label="Time format"
      value={prefs.timeFormat}
      options={timeFormatOptions}
      onchange={(v) => prefs.set('timeFormat', v as TimeFormat)}
    />
  </SettingGroup>
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-layout-gutter);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .row-label {
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }

  .padded {
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
  }

  /* Previews: fixed frame so every layout card is the same width. */
  .preview {
    display: flex;
    width: 15rem;
    max-width: 100%;
    padding: var(--alloy-spacing-scale-3);
    background: var(--alloy-color-surface-canvas);
    border-radius: var(--alloy-radii-md);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-primary);
    box-sizing: border-box;
  }

  .modern {
    gap: var(--alloy-spacing-scale-2);
    align-items: flex-start;
  }

  .pv-avatar {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-accent-primary);
    font-weight: var(--alloy-typography-weight-semibold);
  }

  .pv-body {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }

  .pv-meta {
    display: flex;
    align-items: baseline;
    gap: var(--alloy-spacing-scale-2);
  }

  .pv-name {
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-accent-primary);
  }

  .pv-time {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }

  .pv-text {
    overflow-wrap: anywhere;
  }

  .bubbles {
    justify-content: flex-start;
  }

  .pv-bubble {
    max-width: 100%;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-bubble-other-background);
    color: var(--alloy-color-bubble-other-text);
  }

  .irc {
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-2);
    align-items: baseline;
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-typography-size-xs);
  }

  .pv-nick {
    color: var(--alloy-color-accent-primary);
  }

  .irc .pv-time {
    color: var(--alloy-color-text-muted);
  }
</style>
