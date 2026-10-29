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
    { value: 'bubbles', label: 'Bubbles', description: 'Chat bubbles (default)', preview: bubblesPreview },
    { value: 'modern', label: 'Modern', description: 'Flat rows with avatars', preview: modernPreview },
    { value: 'irc', label: 'IRC', description: 'Compact, single line', preview: ircPreview },
  ];

  // Choosing a theme is an explicit answer, so the store drops follow-system for us. Pure black is
  // the one switch that still needs a precondition of its own.
  const amoledReason = $derived(!dark ? 'Turn on dark mode first.' : undefined);
</script>

{#snippet modernPreview()}
  <span class="pv pv-modern" aria-hidden="true">
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
  <span class="pv pv-bubbles" aria-hidden="true">
    <span class="pv-bubble">Hey you. You're the best!</span>
  </span>
{/snippet}

{#snippet ircPreview()}
  <span class="pv pv-irc" aria-hidden="true">
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
    <!-- Indented under follow-system: they refine the same choice, and touching either one makes it
         an explicit choice of its own. -->
    <div class="theme-children">
      <SettingToggle label="Dark mode" checked={dark} onchange={setDark} />
      <SettingToggle
        label="Pure black background"
        checked={amoled}
        disabled={!dark}
        disabledReason={amoledReason}
        onchange={setAmoled}
      />
    </div>
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
      label="Send and show read receipts"
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

  /* Child switches of the theme group sit inside an indented rail. */
  .theme-children {
    margin-left: var(--alloy-spacing-scale-4);
    padding-left: var(--alloy-spacing-scale-3);
    border-left: 2px solid var(--alloy-color-border-subtle);
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

  /* Previews fill the tile the parent control provides; they paint no background of their own,
     so a layout card reads as one surface rather than a box inside a box. */
  .pv {
    display: flex;
    width: 100%;
    min-width: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-primary);
  }

  .pv-modern {
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

  .pv-bubbles {
    justify-content: flex-start;
  }

  /* Same shape language as the timeline at preview scale: round on three corners, a small tail
     on the sender side, so the choice previews the layout it actually applies. */
  .pv-bubble {
    max-width: 100%;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border-radius: var(--alloy-radii-xl);
    border-bottom-left-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-bubble-other-background);
    color: var(--alloy-color-bubble-other-text);
  }

  .pv-irc {
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    align-items: baseline;
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-typography-size-xs);
  }

  .pv-nick {
    color: var(--alloy-color-accent-primary);
  }
</style>
