<script lang="ts">
  import { SegmentedControl } from '$lib/components/common';
  import type { Snippet } from 'svelte';
  import {
    ChatBackground,
    SettingGroup,
    SettingRadioCards,
    SettingRow,
    SettingSelect,
    SettingToggle,
  } from '$lib/components/settings/controls';
  import { prefs, type FontScale, type MessageLayout, type TimeFormat } from '$lib/stores/prefs.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import themesManifest from '$lib/theme/generated/themes.json';
  import { themeStore } from '$lib/theme/theme.svelte';
  import type { Density, Theme } from '$lib/theme/theme.svelte';

  import { t } from '$lib/i18n';
  const dark = $derived(
    themeStore.activeTheme !== 'alloy-light' && themeStore.activeTheme !== 'solarized-sand',
  );
  const amoled = $derived(
    themeStore.activeTheme === 'alloy-amoled' || themeStore.activeTheme === 'cyber-amoled',
  );

  /** Chats carrying a picture of their own, named the way the room list names them. */
  const backgroundRooms = $derived(
    prefs.backgroundRoomIds.map((id) => {
      const room = roomsStore.rooms.find((r) => r.id === id);
      return { id, title: room ? roomTitle(room) : id };
    }),
  );

  function setDark(on: boolean) {
    themeStore.setTheme(on ? (amoled ? 'alloy-amoled' : 'alloy-dark') : 'alloy-light');
  }
  function setAmoled(on: boolean) {
    themeStore.setTheme(on ? 'alloy-amoled' : 'alloy-dark');
  }

  const densityOptions: { value: Density; label: string }[] = [
    { value: 'compact', label: t('appearance-compact') },
    { value: 'normal', label: t('appearance-normal') },
    { value: 'cozy', label: t('appearance-cozy') },
  ];

  const THEME_ORDER: Record<string, number> = {
    'alloy-light': 0,
    'solarized-sand': 1,
    'alloy-dark': 2,
    'midnight-ocean': 3,
    'nord-frost': 4,
    'dracula-night': 5,
    'catppuccin-mocha': 6,
    'rose-pine': 7,
    'emerald-forest': 8,
    'tokyo-sunset': 9,
    'alloy-amoled': 10,
    'cyber-amoled': 11,
  };

  const themeOptions = [...themesManifest]
    .sort((a, b) => (THEME_ORDER[a.id] ?? 99) - (THEME_ORDER[b.id] ?? 99))
    .map((entry) => ({ value: entry.id, label: entry.name }));

  const fontScaleOptions = [90, 100, 110, 120, 130].map((scale) => ({
    value: String(scale),
    label: scale === 100 ? t('appearance-font-scale-default', { scale }) : `${scale}%`,
  }));

  const timeFormatOptions: { value: TimeFormat; label: string }[] = [
    { value: 'auto', label: t('appearance-automatic') },
    { value: '12h', label: t('appearance-12-hour') },
    { value: '24h', label: t('appearance-24-hour') },
  ];

  // Previews are static markup; they are hoisted, so the option list can reference them.
  const layoutOptions: { value: MessageLayout; label: string; description: string; preview: Snippet }[] = [
    { value: 'bubbles', label: t('appearance-bubbles'), description: t('appearance-chat-bubbles-default'), preview: bubblesPreview },
    { value: 'modern', label: t('appearance-modern'), description: t('appearance-flat-rows-with-avatars'), preview: modernPreview },
    { value: 'irc', label: 'IRC', description: t('appearance-compact-single-line'), preview: ircPreview },
  ];

  // Choosing a theme is an explicit answer, so the store drops follow-system for us. Pure black is
  // the one switch that still needs a precondition of its own.
  const amoledReason = $derived(!dark ? t('appearance-turn-on-dark-mode-first') : undefined);
</script>

{#snippet modernPreview()}
  <span class="pv pv-modern" aria-hidden="true">
    <span class="pv-avatar">A</span>
    <span class="pv-body">
      <span class="pv-meta">
        <span class="pv-name">{t('appearance-alex')}</span>
        <span class="pv-time">10:42</span>
      </span>
      <span class="pv-text">{t('appearance-hey-you-youre-the-best')}</span>
    </span>
  </span>
{/snippet}

{#snippet bubblesPreview()}
  <span class="pv pv-bubbles" aria-hidden="true">
    <span class="pv-bubble">{t('appearance-hey-you-youre-the-best')}</span>
  </span>
{/snippet}

{#snippet ircPreview()}
  <span class="pv pv-irc" aria-hidden="true">
    <span class="pv-time">10:42</span>
    <span class="pv-nick">{'<'}{t('appearance-alex')}{'>'}</span>
    <span class="pv-text">{t('appearance-hey-you-youre-the-best')}</span>
  </span>
{/snippet}

<div class="section">
  <SettingGroup title={t('appearance-theme')}>
    <SettingToggle
      label={t('appearance-follow-system-appearance')}
      checked={themeStore.followSystem}
      onchange={(v) => themeStore.setFollowSystem(v)}
    />
    <!-- Indented under follow-system: they refine the same choice, and touching either one makes it
         an explicit choice of its own. -->
    <div class="theme-children">
      <SettingToggle label={t('appearance-dark-mode')} checked={dark} onchange={setDark} />
      <SettingToggle
        label={t('appearance-pure-black-background')}
        checked={amoled}
        disabled={!dark}
        disabledReason={amoledReason}
        onchange={setAmoled}
      />
      <SettingSelect
        label={t('appearance-theme')}
        value={themeStore.activeTheme}
        options={themeOptions}
        onchange={(v) => themeStore.setTheme(v as Theme)}
      />
      <div class="theme-grid" role="radiogroup" aria-label={t('appearance-theme')}>
        {#each themeOptions as opt (opt.value)}
          {@const selected = themeStore.activeTheme === opt.value}
          <button
            type="button"
            class="theme-card"
            class:selected
            role="radio"
            aria-checked={selected}
            onclick={() => themeStore.setTheme(opt.value as Theme)}
          >
            <span class="theme-swatch" data-theme={opt.value} aria-hidden="true">
              <span class="swatch-panel"></span>
              <span class="swatch-canvas">
                <span class="swatch-bubble"></span>
                <span class="swatch-accent"></span>
              </span>
            </span>
            <span class="theme-card-footer">
              <span class="theme-name">{opt.label}</span>
              {#if selected}
                <span class="theme-dot" aria-hidden="true"></span>
              {/if}
            </span>
          </button>
        {/each}
      </div>
    </div>
    <SettingRow label={t('appearance-density')}>
      <SegmentedControl
        label={t('appearance-density')}
        options={densityOptions}
        value={themeStore.density}
        onchange={(v) => themeStore.setDensity(v as Density)}
      />
    </SettingRow>
  </SettingGroup>

  <SettingGroup
    title={t('chat-background-title')}
    description={t('chat-background-description')}
  >
    <ChatBackground roomId={null} />
    {#if backgroundRooms.length > 0}
      <div class="own-rooms">
        <p class="own-rooms-title">{t('chat-background-chats-with-own')}</p>
        <ul class="own-rooms-list">
          {#each backgroundRooms as room (room.id)}
            <li class="own-room">{room.title}</li>
          {/each}
        </ul>
      </div>
    {/if}
  </SettingGroup>

  <SettingGroup title={t('appearance-messages')}>
    <div class="padded">
      <SettingRadioCards
        legend={t('appearance-message-layout')}
        value={prefs.messageLayout}
        options={layoutOptions}
        onchange={(v) => prefs.set('messageLayout', v as MessageLayout)}
      />
    </div>
    <SettingToggle
      label={t('appearance-enable-big-emoji-in-chat')}
      checked={prefs.bigEmoji}
      onchange={(v) => prefs.set('bigEmoji', v)}
    />
  </SettingGroup>

  <SettingGroup title={t('appearance-text-and-time')}>
    <SettingSelect
      label={t('appearance-font-size')}
      value={String(prefs.fontScale)}
      options={fontScaleOptions}
      onchange={(v) => prefs.set('fontScale', Number(v) as FontScale)}
    />
    <SettingSelect
      label={t('appearance-time-format')}
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
    gap: var(--alloy-spacing-scale-6);
  }

  /* Child switches of the theme group sit inside an indented rail. */
  .theme-children {
    margin-left: var(--alloy-spacing-scale-4);
    padding-left: var(--alloy-spacing-scale-3);
    border-left: 2px solid var(--alloy-color-border-subtle);
  }

  .theme-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(8.5rem, 1fr));
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2) 0 var(--alloy-spacing-scale-3);
  }

  .theme-card {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    padding: var(--alloy-spacing-scale-2);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-primary);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      border-color var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      background-color var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      box-shadow var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease);
  }

  @media (hover: hover) {
    .theme-card:hover {
      background: var(--alloy-color-surface-hover);
      border-color: var(--alloy-color-border-prominent);
    }
  }

  .theme-card.selected {
    border-color: var(--alloy-color-accent-primary);
    box-shadow: inset 0 0 0 1px var(--alloy-color-accent-primary);
    background: color-mix(
      in srgb,
      var(--alloy-color-accent-subtle) 45%,
      var(--alloy-color-surface-subtle)
    );
  }

  .theme-card:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }

  /* Mini preview inherits --alloy-color-* directly from [data-theme="..."] in themes.css. */
  .theme-swatch {
    display: grid;
    grid-template-columns: 28% 1fr;
    height: 2.5rem;
    overflow: hidden;
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-canvas);
  }

  .swatch-panel {
    background: var(--alloy-color-surface-panel);
    border-right: 1px solid var(--alloy-color-border-subtle);
  }

  .swatch-canvas {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
    padding: var(--alloy-spacing-scale-1);
    background: var(--alloy-color-surface-canvas);
  }

  .swatch-bubble {
    align-self: flex-start;
    width: 58%;
    height: 7px;
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-bubble-other-background);
  }

  .swatch-accent {
    align-self: flex-end;
    width: 52%;
    height: 7px;
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-accent-primary);
  }

  .theme-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }

  .theme-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-font-size-preview);
    font-weight: var(--alloy-typography-weight-medium);
    line-height: var(--alloy-typography-line-height-tight);
    color: var(--alloy-color-text-primary);
  }

  .theme-card.selected .theme-name {
    font-weight: var(--alloy-typography-weight-semibold);
  }

  .theme-dot {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
  }

  .padded {
    padding: var(--alloy-spacing-scale-2) 0;
  }

  /* The chats that carry a picture of their own: a read-only list, because each one is changed
     from its own picker (the room menu) where its picture is previewed. */
  .own-rooms {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    padding-top: var(--alloy-spacing-scale-3);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .own-rooms-title {
    margin: 0;
    font-size: var(--alloy-font-size-meta);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-letter-spacing-caps);
    text-transform: uppercase;
    color: var(--alloy-color-text-muted);
  }

  .own-rooms-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }

  .own-room {
    font-size: var(--alloy-font-size-row);
    color: var(--alloy-color-text-primary);
  }

  /* Previews fill the tile the parent control provides; they paint no background of their own,
     so a layout card reads as one surface rather than a box inside a box. */
  .pv {
    display: flex;
    width: 100%;
    min-width: 0;
    font-size: var(--alloy-font-size-preview);
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
    border-radius: var(--alloy-bubble-radius);
    border-bottom-left-radius: var(--alloy-bubble-radius-tail);
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
