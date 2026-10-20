<script lang="ts">
  import { onMount, tick, type Component } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import {
    applySkinTone,
    loadEmojiData,
    SKIN_TONES,
    searchEmoji,
    type EmojiData,
    type EmojiEntry,
    type SkinTone
  } from '$lib/content/emoji';
  import { getSkinTone, recentEmoji, recordEmoji, setSkinTone } from '$lib/stores/emojiRecents';
  import { contextMenu, type MenuAction } from '$lib/stores/contextmenu.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import {
    Apple,
    Clock,
    Flag,
    Icon,
    Lightbulb,
    PawPrint,
    Plane,
    Search,
    Shapes,
    Smile,
    Trophy,
    Users,
    X
  } from '$lib/icons';
  import TextField from './TextField.svelte';

  type IconComponent = Component<{ size?: number; strokeWidth?: number; class?: string }>;

  interface Props {
    /** Receives the chosen emoji (already toned). Recording it in Recents happens here. */
    onpick: (emoji: string) => void;
    onclose?: () => void;
    /** Focus the search field on mount (desktop). Phones leave the keyboard down. */
    autofocus?: boolean;
    /** Called on phones when search input is focused or blurred. */
    onsearchfocus?: (focused: boolean) => void;
    /** Called on phones when search query becomes active or clears. */
    onsearching?: (active: boolean) => void;
  }

  let { onpick, onclose, autofocus = false, onsearchfocus, onsearching }: Props = $props();

  /** A renderable emoji: the display glyph for the current tone plus its label. */
  interface Cell {
    emoji: string;
    label: string;
  }

  interface Section {
    key: string;
    label: string;
    cells: Cell[];
  }

  interface Tab {
    key: string;
    label: string;
    icon: IconComponent;
  }

  /** Category icons keyed by emojibase group key. Unknown groups fall back to Smile. */
  const GROUP_ICONS: Record<string, IconComponent> = {
    'smileys-emotion': Smile,
    'people-body': Users,
    'animals-nature': PawPrint,
    'food-drink': Apple,
    'travel-places': Plane,
    activities: Trophy,
    objects: Lightbulb,
    symbols: Shapes,
    flags: Flag
  };

  const RECENT_KEY = 'recent';
  /** Rough row heights for placeholders of sections not yet rendered, so the scrollbar stays honest. */
  const CELL_HEIGHT_DESKTOP = 40;
  const CELL_HEIGHT_PHONE = 46;
  const COLUMNS_DESKTOP = 9;
  const COLUMNS_PHONE = 7;

  let data = $state<EmojiData | null>(null);
  let loadFailed = $state(false);
  let query = $state('');
  let tone = $state<SkinTone>(getSkinTone());
  let recents = $state(recentEmoji());
  let activeKey = $state('');
  let preview = $state<Cell | null>(null);
  let scroller = $state<HTMLElement | null>(null);
  let searchEl = $state<HTMLInputElement | null>(null);

  /** Section DOM nodes by key, used for tab jumps, scroll spy and lazy observation. */
  const sectionEls: Record<string, HTMLElement | undefined> = {};
  /** Group keys whose emoji grids have been rendered; the rest show placeholders. */
  const rendered = new SvelteSet<string>();

  let searchFocused = $state(false);
  const searching = $derived(query.trim() !== '');
  const hideTabs = $derived(viewport.isPhone && (searchFocused || searching));

  $effect(() => {
    onsearching?.(searching);
  });

  function onSearchFocus() {
    searchFocused = true;
    onsearchfocus?.(true);
    if (typeof window !== 'undefined' && (window.scrollX !== 0 || window.scrollY !== 0)) {
      window.scrollTo(0, 0);
    }
  }

  function onSearchBlur() {
    searchFocused = false;
    onsearchfocus?.(false);
  }

  function toCell(entry: EmojiEntry): Cell {
    const shown = applySkinTone(entry, tone);
    return { emoji: shown.emoji, label: shown.label };
  }

  const sections = $derived<Section[]>(
    data
      ? data.groups.map((g) => ({
          key: g.key,
          label: g.label,
          cells: g.emojis.map(toCell)
        }))
      : []
  );

  const recentCells = $derived.by<Cell[]>(() => {
    if (!data) return [];
    const byEmoji = new Map<string, EmojiEntry>();
    for (const entry of data.all) {
      byEmoji.set(entry.emoji, entry);
      for (const skin of entry.skins) byEmoji.set(skin.emoji, skin);
    }
    const cells: Cell[] = [];
    for (const r of recents) {
      const entry = byEmoji.get(r.emoji);
      if (entry) cells.push({ emoji: entry.emoji, label: entry.label });
    }
    return cells;
  });

  const results = $derived<Cell[]>(
    data && searching ? searchEmoji(data.all, query).map(toCell) : []
  );

  const tabs = $derived<Tab[]>([
    ...(recentCells.length > 0
      ? [{ key: RECENT_KEY, label: 'Recently used', icon: Clock } satisfies Tab]
      : []),
    ...sections.map((s) => ({
      key: s.key,
      label: s.label,
      icon: GROUP_ICONS[s.key] ?? Smile
    }))
  ]);

  const currentTab = $derived(
    tabs.some((t) => t.key === activeKey) ? activeKey : (tabs[0]?.key ?? '')
  );

  const toneOption = $derived(SKIN_TONES.find((t) => t.id === tone) ?? SKIN_TONES[0]);

  function shortcode(label: string): string {
    return label.replace(/:/g, '').trim().replace(/\s+/g, '_');
  }

  function choose(emoji: string) {
    recordEmoji(emoji);
    recents = recentEmoji();
    onpick(emoji);
  }

  function jumpTo(key: string) {
    if (searching) {
      query = '';
    }
    activeKey = key;
    tick().then(() => {
      const root = scroller;
      const el = sectionEls[key];
      if (!root || !el) return;
      rendered.add(key);
      root.scrollTo({ top: el.offsetTop, behavior: 'auto' });
    });
  }

  /** Scroll spy: the last section whose top has reached the scroller's top edge is active. */
  function onScroll() {
    const root = scroller;
    if (!root || searching) return;
    const line = root.scrollTop + 4;
    let current = tabs[0]?.key ?? '';
    for (const tab of tabs) {
      const el = sectionEls[tab.key];
      if (el && el.offsetTop <= line) current = tab.key;
    }
    activeKey = current;
  }

  // Lazy sections: a section's grid renders once it comes within reach of the scroller.
  $effect(() => {
    const root = scroller;
    const loaded = data !== null;
    const browsing = !searching;
    if (!root || !loaded || !browsing) return;
    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const key = (entry.target as HTMLElement).dataset.key;
          if (entry.isIntersecting && key) rendered.add(key);
        }
      },
      { root, rootMargin: '400px 0px' }
    );
    for (const el of Object.values(sectionEls)) {
      if (el?.isConnected) io.observe(el);
    }
    return () => io.disconnect();
  });

  onMount(() => {
    let live = true;
    loadEmojiData().then(
      (loaded) => {
        if (live) data = loaded;
      },
      () => {
        if (live) loadFailed = true;
      }
    );
    if (autofocus) searchEl?.focus({ preventScroll: true });
    return () => {
      live = false;
    };
  });

  // Once data is in, the first section should be visible without waiting for an observer pass.
  $effect(() => {
    if (data && tabs.length > 0 && activeKey === '') activeKey = tabs[0].key;
  });

  function cellButtons(root: HTMLElement): HTMLButtonElement[] {
    return [...root.querySelectorAll<HTMLButtonElement>('button[data-cell]')];
  }

  function onGridKey(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    if (!target.matches('button[data-cell]')) return;
    const buttons = cellButtons(e.currentTarget as HTMLElement);
    const index = buttons.indexOf(target as HTMLButtonElement);
    const grid = target.parentElement;
    const columns = grid
      ? getComputedStyle(grid).gridTemplateColumns.split(' ').filter(Boolean).length
      : COLUMNS_DESKTOP;
    let next: number;
    switch (e.key) {
      case 'ArrowRight':
        next = index + 1;
        break;
      case 'ArrowLeft':
        next = index - 1;
        break;
      case 'ArrowDown':
        next = index + columns;
        break;
      case 'ArrowUp':
        if (index - columns < 0) {
          e.preventDefault();
          searchEl?.focus({ preventScroll: true });
          return;
        }
        next = index - columns;
        break;
      default:
        return;
    }
    if (next < 0 || next >= buttons.length) return;
    e.preventDefault();
    buttons[next].focus();
  }

  function onRootKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose?.();
      return;
    }
    const inSearch = e.target === searchEl;
    if (inSearch) {
      if (e.key === 'ArrowDown') {
        const first = scroller?.querySelector<HTMLButtonElement>('button[data-cell]');
        if (first) {
          e.preventDefault();
          first.focus();
        }
      }
      return;
    }
    if (e.target instanceof HTMLElement && e.target.matches('button[data-cell]')) {
      onGridKey(e);
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === '/' || (e.key.length === 1 && e.key !== ' ')) {
      e.preventDefault();
      if (e.key !== '/') query += e.key;
      searchEl?.focus({ preventScroll: true });
    }
  }

  function clearSearch() {
    query = '';
    searchEl?.focus({ preventScroll: true });
  }

  function openToneMenu(e: MouseEvent) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items: MenuAction[] = SKIN_TONES.map((t) => ({
      id: t.id,
      label: `${t.swatch} ${t.label}`,
      selected: t.id === tone,
      onselect: () => {
        tone = t.id;
        setSkinTone(t.id);
      }
    }));
    contextMenu.open(items, { x: rect.left, y: rect.top }, {
      anchor: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
      ariaLabel: 'Skin tone'
    });
  }

  function estimatedHeight(count: number): number {
    const phone = viewport.isPhone;
    const columns = phone ? COLUMNS_PHONE : COLUMNS_DESKTOP;
    const rowHeight = phone ? CELL_HEIGHT_PHONE : CELL_HEIGHT_DESKTOP;
    return Math.ceil(count / columns) * rowHeight;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="emoji-picker" onkeydown={onRootKey} role="dialog" aria-label="Emoji picker" tabindex="-1">
  <div class="top">
    <div class="tabs" role="tablist" aria-label="Emoji categories" class:hidden={hideTabs}>
      {#each tabs as tab (tab.key)}
        <button
          type="button"
          role="tab"
          class="tab"
          class:active={currentTab === tab.key && !searching}
          aria-selected={currentTab === tab.key && !searching}
          aria-label={tab.label}
          title={tab.label}
          onclick={() => jumpTo(tab.key)}
        >
          <Icon icon={tab.icon} size={20} />
        </button>
      {/each}
    </div>

    <div class="search">
      <TextField
        bind:el={searchEl}
        bind:value={query}
        icon={Search}
        placeholder="Search Emoji"
        aria-label="Search emoji"
        autocomplete="off"
        spellcheck={false}
        class="search-input"
        onfocus={onSearchFocus}
        onblur={onSearchBlur}
      />
      {#if query !== ''}
        <button
          type="button"
          class="clear"
          aria-label="Clear search"
          title="Clear search"
          onclick={clearSearch}
        >
          <Icon icon={X} size={16} />
        </button>
      {/if}
    </div>
  </div>

  <div class="scroller" bind:this={scroller} onscroll={onScroll}>
    {#snippet cellButton(c: Cell)}
      <button
        type="button"
        class="cell"
        data-cell
        aria-label={c.label}
        onclick={() => choose(c.emoji)}
        onpointerenter={() => (preview = c)}
        onpointerleave={() => (preview = null)}
        onfocus={() => (preview = c)}
        onblur={() => (preview = null)}
      >
        {c.emoji}
      </button>
    {/snippet}

    {#if data === null}
      <p class="status">{loadFailed ? 'Emoji unavailable' : 'Loading emoji…'}</p>
    {:else if searching}
      {#if results.length === 0}
        <p class="status">No emoji found</p>
      {:else}
        <div class="section">
          <h3 class="heading">Results</h3>
          <div class="grid">
            {#each results as c (c.emoji)}
              {@render cellButton(c)}
            {/each}
          </div>
        </div>
      {/if}
    {:else}
      {#if recentCells.length > 0}
        <section class="section" bind:this={sectionEls[RECENT_KEY]} data-key={RECENT_KEY} aria-labelledby="emoji-h-{RECENT_KEY}">
          <h3 class="heading" id="emoji-h-{RECENT_KEY}">Recently used</h3>
          <div class="grid">
            {#each recentCells as c (c.emoji)}
              {@render cellButton(c)}
            {/each}
          </div>
        </section>
      {/if}

      {#each sections as s (s.key)}
        <section class="section" bind:this={sectionEls[s.key]} data-key={s.key} aria-labelledby="emoji-h-{s.key}">
          <h3 class="heading" id="emoji-h-{s.key}">{s.label}</h3>
          {#if rendered.has(s.key)}
            <div class="grid">
              {#each s.cells as c (c.emoji)}
                {@render cellButton(c)}
              {/each}
            </div>
          {:else}
            <div class="placeholder" style:min-height="{estimatedHeight(s.cells.length)}px"></div>
          {/if}
        </section>
      {/each}
    {/if}
  </div>

  <footer class="footer">
    <div class="preview" aria-hidden="true">
      {#if preview}
        <span class="preview-emoji">{preview.emoji}</span>
        <span class="preview-text">
          <span class="preview-label">{preview.label}</span>
          <span class="preview-code">:{shortcode(preview.label)}:</span>
        </span>
      {/if}
    </div>
    <button
      type="button"
      class="tone"
      title="Skin tone"
      aria-label="Skin tone: {toneOption.label}"
      aria-haspopup="menu"
      onclick={openToneMenu}
    >
      {toneOption.swatch}
    </button>
  </footer>
</div>

<style>
  .emoji-picker {
    display: flex;
    flex-direction: column;
    flex: 1 1 0;
    min-height: 0;
    height: 100%;
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    font-family: var(--alloy-typography-font-family-sans);
    outline: none;
  }

  .top {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
  }

  .tabs {
    display: flex;
    gap: var(--alloy-spacing-scale-1);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tabs::-webkit-scrollbar {
    display: none;
  }

  .tabs.hidden {
    display: none;
  }

  .tab {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: none;
    border-radius: var(--alloy-radii-sm);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .tab:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .tab.active {
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-accent-primary);
  }

  .search {
    position: relative;
  }
  .search :global(.text-field) {
    width: 100%;
    font-size: var(--alloy-typography-size-sm);
  }
  .search :global(.text-field.has-icon) {
    padding-inline-end: var(--alloy-spacing-scale-7);
  }

  .clear {
    position: absolute;
    right: var(--alloy-spacing-scale-1);
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: var(--alloy-radii-sm);
    background: transparent;
    color: var(--alloy-color-text-muted);
    cursor: pointer;
  }
  .clear:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }

  .scroller {
    position: relative;
    flex: 1 1 0;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 0 var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2);
  }

  .heading {
    position: sticky;
    top: 0;
    z-index: 1;
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-1);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(9, minmax(0, 1fr));
    gap: 2px;
  }

  .cell {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 38px;
    border: none;
    border-radius: var(--alloy-radii-sm);
    background: transparent;
    font-size: 22px;
    line-height: 1;
    cursor: pointer;
  }
  .cell:hover,
  .cell:focus-visible {
    background: var(--alloy-color-surface-hover);
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }

  .placeholder {
    width: 100%;
  }

  .status {
    margin: var(--alloy-spacing-scale-4) 0;
    text-align: center;
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-sm);
  }

  .footer {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    min-height: 44px;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .preview {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    flex: 1 1 auto;
  }
  .preview-emoji {
    font-size: 24px;
    line-height: 1;
  }
  .preview-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .preview-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-medium);
  }
  .preview-code {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-xs);
  }

  .tone {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-sm);
    background: transparent;
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
  }
  .tone:hover {
    background: var(--alloy-color-surface-hover);
  }

  /* Phones: 44px targets and a grid sized to fit a 360px-wide sheet without horizontal overflow. */
  @media (max-width: 899px) {
    .tab {
      width: 44px;
      height: 44px;
    }
    .clear {
      width: 44px;
      height: 44px;
    }
    .search :global(.text-field) {
      font-size: 16px;
    }
    .grid {
      grid-template-columns: repeat(7, minmax(44px, 1fr));
    }
    .cell {
      height: 44px;
    }
    .tone {
      width: 44px;
      height: 44px;
    }
    .preview {
      display: none;
    }
    .footer {
      justify-content: flex-end;
    }
  }
</style>
