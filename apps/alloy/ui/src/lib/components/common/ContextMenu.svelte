<script lang="ts">
  import { tick, untrack } from 'svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Check, ChevronLeft, ChevronRight, Ellipsis, Plus } from '$lib/icons';
  import { portal } from '$lib/platform/portal';
  import { pushBack } from '$lib/platform/backstack';
  import { placeAnchor, placeDesktopReactionStrip, placeFlyout, placeMenu, placeReactionBar } from '$lib/platform/contextmenu';
  import {
    contextMenu,
    isSeparator,
    isSubmenuAction,
    resolveSubmenuItems,
    submenuView,
    type MenuAction,
    type MenuAnchor,
    type MenuItem,
    type MenuOptions,
    type MenuPoint,
    type MenuSearchCreate,
    type MenuSubmenu,
    type SubmenuAction,
  } from '$lib/stores/contextmenu.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import TextField from './TextField.svelte';
  import QuickReactions from './QuickReactions.svelte';

  const TYPEAHEAD_RESET_MS = 600;
  const HOVER_OPEN_MS = 150;
  const FLYOUT_MAX_HEIGHT = 320;
  const NAV_SELECTOR = '[role^="menuitem"]:not([aria-disabled="true"])';
  const ROW_SELECTOR = '.row:not([aria-disabled="true"])';

  const CANCEL: MenuAction = { id: 'cancel', label: 'Cancel' };

  /** Stand-in while the menu closes: its options are cleared before the blocks that read them are removed. */
  const NO_REACTIONS: NonNullable<MenuOptions['reactions']> = { emojis: [], active: [], onpick: () => {}, onmore: () => {} };

  /** A submenu flyout. `levels[k]` is opened from a row at depth k and is displayed at depth k + 1. */
  interface Flyout {
    /** Index of the opening row within its panel's rendered rows. */
    parentIndex: number;
    parentEl: HTMLElement;
    label: string;
    sub: MenuSubmenu;
    query: string;
    /** Releases the Android Back handler that closes this flyout. */
    release: () => void;
  }

  interface PanelView {
    rows: MenuItem[];
    query: string;
    showCreate: boolean;
    createRow: MenuAction | undefined;
  }

  interface PlaceParams {
    depth: number;
    /** Re-runs placement when the rendered rows change. */
    rows: MenuItem[];
  }

  let levels = $state.raw<Flyout[]>([]);
  /** Bumped when a keepOpen row runs, so function-based submenu items are resolved again. */
  let revision = $state(0);
  /** Phone shows only the deepest panel (drill-in); desktop shows the whole chain. */
  const renderedLevels = $derived(
    viewport.isPhone ? [levels.length] : Array.from({ length: levels.length + 1 }, (_, i) => i),
  );

  /** Panel element per depth. Not reactive: only used by handlers and placement. */
  const menuEls = new Map<number, HTMLElement>();
  let scrimEl: HTMLDivElement | undefined = $state();
  let bottomBarEl = $state<HTMLElement | undefined>();
  let desktopStripEl = $state<HTMLElement | undefined>();
  let showFullSheet = $state(false);
  let openedAt = $state(0);
  let pointerupAfterOpen = $state(false);
  let typed = '';
  let typedTimer: number | null = null;
  let hoverTimer: number | null = null;

  function createAction(create: MenuSearchCreate, query: string): MenuAction {
    return {
      id: 'create',
      label: create.label(query),
      icon: Plus,
      shortcut: 'Enter',
      onselect: () => create.onCreate(query),
    };
  }

  /** Rows for the panel at `depth`: root items, or a flyout's filtered items plus the create row. */
  function panelView(depth: number): PanelView {
    void revision;
    if (depth === 0) {
      return { rows: contextMenu.items, query: '', showCreate: false, createRow: undefined };
    }
    const flyout = levels[depth - 1];
    const create = flyout.sub.search?.create;
    const view = submenuView(resolveSubmenuItems(flyout.sub), flyout.query, create !== undefined);
    const createRow = view.showCreate && create ? createAction(create, view.query) : undefined;
    return {
      rows: createRow ? [...view.items, createRow] : view.items,
      query: view.query,
      showCreate: view.showCreate,
      createRow,
    };
  }

  function enabledIn(depth: number, selector: string): HTMLElement[] {
    return Array.from(menuEls.get(depth)?.querySelectorAll<HTMLElement>(selector) ?? []);
  }

  /** Position a panel: flyouts beside their parent row, the root at its anchor or pointer point. */
  function placeLevel(node: HTMLElement, params: PlaceParams) {
    let depth = params.depth;
    menuEls.set(depth, node);
    const apply = () => {
      // Phone: the menu is a bottom sheet placed by CSS, so no coordinates are applied.
      if (viewport.isPhone) return;
      const parent = levels[depth - 1];
      if (depth > 0 && !parent) return;
      if (!parent) {
        const opts = contextMenu.options;
        if (opts?.anchor && opts.matchWidth) {
          node.style.width = `${opts.anchor.width}px`;
          node.style.minWidth = `${opts.anchor.width}px`;
          node.style.maxWidth = 'none';
        } else if (opts?.minWidth) {
          node.style.minWidth = `${opts.minWidth}px`;
          node.style.maxWidth = '';
          node.style.width = '';
        } else {
          node.style.width = '';
          node.style.minWidth = '';
          node.style.maxWidth = '';
        }
      }
      const size = { width: node.offsetWidth, height: node.offsetHeight };
      const screen = { width: window.innerWidth, height: window.innerHeight };
      const anchor = contextMenu.options?.anchor;
      let p: MenuPoint;
      if (parent) {
        p = placeFlyout(parent.parentEl.getBoundingClientRect(), size, screen);
      } else if (anchor) {
        p = placeAnchor(anchor, size, screen);
      } else {
        p = placeMenu(contextMenu.at, size, screen);
      }
      if (!parent && !viewport.isPhone && contextMenu.options?.reactions) {
        const stripSpace = 44;
        if (p.y < stripSpace + 8) {
          p.y = Math.min(screen.height - size.height - 8, stripSpace + 8);
        }
      }
      node.style.left = `${p.x}px`;
      node.style.top = `${p.y}px`;
    };
    apply();
    return {
      update(next: PlaceParams) {
        depth = next.depth;
        apply();
      },
      destroy() {
        if (menuEls.get(depth) === node) menuEls.delete(depth);
      },
    };
  }

  /** Phone: float the quick-reaction pill beside the pressed message, clear of the action sheet. */
  function placeFloat(node: HTMLElement, anchor: MenuAnchor | undefined) {
    let live = true;
    const apply = async () => {
      await tick();
      if (!live) return;
      const sheetHeight = showFullSheet || !contextMenu.options?.phoneBar
        ? (menuEls.get(0)?.offsetHeight ?? 0)
        : (bottomBarEl?.offsetHeight ?? 72);
      const inset = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--alloy-safe-area-top')) || 0;
      const screen = { width: window.innerWidth, height: window.innerHeight };
      const target = anchor ?? { x: 0, y: screen.height / 2, width: screen.width, height: 0 };
      const p = placeReactionBar(
        target,
        { width: node.offsetWidth, height: node.offsetHeight },
        screen,
        { top: inset + 56, bottom: screen.height - sheetHeight },
      );
      node.style.left = `${p.x}px`;
      node.style.top = `${p.y}px`;
    };
    void apply();
    return {
      update(next: MenuAnchor | undefined) {
        anchor = next;
        void apply();
      },
      destroy() {
        live = false;
      },
    };
  }

  /** Desktop: float the quick-reaction strip directly above the root menu card. */
  function placeDesktopStrip(node: HTMLElement) {
    let live = true;
    const apply = async () => {
      await tick();
      if (!live) return;
      const card = menuEls.get(0);
      if (!card) return;
      const cardRect = card.getBoundingClientRect();
      const cardPoint = { x: cardRect.left, y: cardRect.top };
      const cardSize = { width: cardRect.width, height: cardRect.height };
      const stripSize = { width: node.offsetWidth, height: node.offsetHeight };
      const screen = { width: window.innerWidth, height: window.innerHeight };
      const p = placeDesktopReactionStrip(cardPoint, cardSize, stripSize, screen);
      node.style.left = `${p.x}px`;
      node.style.top = `${p.y}px`;
    };
    void apply();
    return {
      update() {
        void apply();
      },
      destroy() {
        live = false;
      },
    };
  }

  function focusStep(els: HTMLElement[], current: HTMLElement, delta: 1 | -1) {
    if (els.length === 0) return;
    const i = els.indexOf(current);
    const next = i < 0 ? (delta > 0 ? 0 : els.length - 1) : (i + delta + els.length) % els.length;
    els[next].focus({ preventScroll: true });
  }

  function typeahead(els: HTMLElement[], current: HTMLElement, key: string) {
    if (typedTimer !== null) window.clearTimeout(typedTimer);
    typed += key.toLowerCase();
    typedTimer = window.setTimeout(() => {
      typed = '';
      typedTimer = null;
    }, TYPEAHEAD_RESET_MS);
    const start = els.indexOf(current);
    for (let k = 1; k <= els.length; k++) {
      const el = els[(start + k) % els.length];
      if (el.textContent?.trim().toLowerCase().startsWith(typed)) {
        el.focus({ preventScroll: true });
        return;
      }
    }
  }

  function trimLevels(keep: number) {
    if (levels.length <= keep) return;
    for (const level of levels.slice(keep)) level.release();
    levels = levels.slice(0, keep);
  }

  /** Close the flyout displayed at `depth` and return focus to the row that opened it. */
  function closeLevel(depth: number) {
    const parent = levels[depth - 1];
    if (!parent) return;
    trimLevels(depth - 1);
    if (viewport.isPhone) {
      tick().then(() => focusEntry(depth - 1));
    } else {
      parent.parentEl.focus({ preventScroll: true });
    }
  }

  /**
   * Open the flyout for submenu row `idx` (element `el`) in the panel at depth `k`. Idempotent for the
   * same row. `focus` moves focus into the flyout; hover passes false so the pointer does not steal focus.
   */
  function openChild(k: number, idx: number, el: HTMLElement, item: SubmenuAction, focus: boolean) {
    const open = levels[k];
    if (open?.parentIndex === idx && open.parentEl === el) {
      if (focus) focusEntry(k + 1);
      return;
    }
    trimLevels(k);
    const release = pushBack(() => closeLevel(k + 1));
    levels = [
      ...levels,
      { parentIndex: idx, parentEl: el, label: item.label, sub: item.submenu, query: '', release },
    ];
    if (focus) tick().then(() => focusEntry(k + 1));
  }

  /** Focus the search field (desktop flyouts with search), else the selected or first enabled row. */
  function focusEntry(depth: number) {
    const panel = menuEls.get(depth);
    if (!panel) return;
    const search = viewport.isPhone ? null : panel.querySelector<HTMLInputElement>('.search input');
    if (search) {
      search.focus({ preventScroll: true });
      return;
    }
    const selected = panel.querySelector<HTMLElement>(
      '[role="menuitemradio"][aria-checked="true"]:not([aria-disabled="true"])',
    );
    const target = selected ?? enabledIn(depth, ROW_SELECTOR)[0];
    target?.focus({ preventScroll: true });
  }

  function cancelHover() {
    if (hoverTimer === null) return;
    window.clearTimeout(hoverTimer);
    hoverTimer = null;
  }

  function enterRow(e: PointerEvent, depth: number, idx: number, item: MenuAction) {
    if (item.disabled) return;
    const el = e.currentTarget as HTMLElement;
    cancelHover();
    hoverTimer = window.setTimeout(() => {
      hoverTimer = null;
      if (isSubmenuAction(item)) openChild(depth, idx, el, item, false);
      else trimLevels(depth);
    }, HOVER_OPEN_MS);
  }

  function focusOnMove(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    if (el.getAttribute('aria-disabled') === 'true') return;
    const panel = el.closest('[data-context-menu]');
    if (panel?.querySelector('.search input') === document.activeElement) return;
    el.focus({ preventScroll: true });
  }

  function clickRow(e: MouseEvent, depth: number, idx: number, item: MenuAction) {
    if (item.disabled) return;
    if (isSubmenuAction(item)) {
      openChild(depth, idx, e.currentTarget as HTMLElement, item, true);
      return;
    }
    selectByPointer(item);
  }

  /** Run a row's action. keepOpen rows stay open and re-resolve function-based submenu items. */
  function runSelect(item: MenuAction, closeFirst: () => void) {
    if (item.disabled) return;
    if (item.keepOpen) {
      item.onselect?.();
      revision++;
      return;
    }
    closeFirst();
    item.onselect?.();
  }

  /** Pointer activation: close first so the menu is gone before the action runs. */
  function selectByPointer(item: MenuAction) {
    runSelect(item, () => contextMenu.close());
  }

  /** Keyboard activation: restore focus to the opener first, then run the action. */
  function selectByKeyboard(item: MenuAction) {
    runSelect(item, () => contextMenu.dismiss());
  }

  function typeIntoSearch(depth: number, key: string) {
    const query = (levels[depth - 1]?.query ?? '') + key;
    onSearchInput(depth, query);
    tick().then(() => {
      const input = menuEls.get(depth)?.querySelector<HTMLInputElement>('.search input');
      input?.focus({ preventScroll: true });
      input?.setSelectionRange(query.length, query.length);
    });
  }

  /** Arrow, Home/End, ArrowLeft and typeahead for rows in the panel at `depth`. */
  function onNavKeydown(e: KeyboardEvent, depth: number) {
    const el = e.currentTarget as HTMLElement;
    const els = enabledIn(depth, NAV_SELECTOR);
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        focusStep(els, el, 1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        focusStep(els, el, -1);
        break;
      case 'Home':
        e.preventDefault();
        els[0]?.focus({ preventScroll: true });
        break;
      case 'End':
        e.preventDefault();
        els[els.length - 1]?.focus({ preventScroll: true });
        break;
      case 'ArrowLeft':
        if (depth > 0) {
          e.preventDefault();
          closeLevel(depth);
        }
        break;
      default:
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
          e.preventDefault();
          if (levels[depth - 1]?.sub.search) typeIntoSearch(depth, e.key);
          else typeahead(els, el, e.key);
        }
    }
  }

  function onRowKeydown(e: KeyboardEvent, depth: number, idx: number, item: MenuAction) {
    const el = e.currentTarget as HTMLElement;
    if (e.key === 'ArrowRight' && isSubmenuAction(item)) {
      e.preventDefault();
      openChild(depth, idx, el, item, true);
      return;
    }
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      if (isSubmenuAction(item)) openChild(depth, idx, el, item, true);
      else selectByKeyboard(item);
      return;
    }
    onNavKeydown(e, depth);
  }

  /** Back and Cancel chrome rows: Enter/Space runs `activate`, everything else navigates. */
  function onChromeKeydown(e: KeyboardEvent, depth: number, activate: () => void) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      activate();
      return;
    }
    onNavKeydown(e, depth);
  }

  function onSearchInput(depth: number, query: string) {
    trimLevels(depth);
    levels = levels.map((level, i) => (i === depth - 1 ? { ...level, query } : level));
  }

  function onSearchKeydown(e: KeyboardEvent, depth: number) {
    const input = e.currentTarget as HTMLInputElement;
    const rows = enabledIn(depth, ROW_SELECTOR);
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        rows[0]?.focus({ preventScroll: true });
        break;
      case 'ArrowUp':
        e.preventDefault();
        rows[rows.length - 1]?.focus({ preventScroll: true });
        break;
      case 'Enter': {
        e.preventDefault();
        const view = panelView(depth);
        // With no matches the only row is the create row.
        if (view.createRow && view.rows.length === 1) selectByKeyboard(view.createRow);
        else rows[0]?.focus({ preventScroll: true });
        break;
      }
      case 'ArrowLeft':
        if (input.selectionStart === 0 && input.selectionEnd === 0) {
          e.preventDefault();
          closeLevel(depth);
        }
        break;
    }
  }

  // Focus the root's entry point whenever the menu opens or its items are replaced; any open flyouts close.
  $effect(() => {
    void contextMenu.items;
    void contextMenu.isOpen;
    untrack(() => trimLevels(0));
    if (!contextMenu.isOpen) {
      showFullSheet = false;
      openedAt = 0;
      pointerupAfterOpen = false;
      return;
    }
    showFullSheet = false;
    openedAt = Date.now();
    pointerupAfterOpen = false;
    const onWindowPointerUp = () => {
      pointerupAfterOpen = true;
      window.removeEventListener('pointerup', onWindowPointerUp, true);
    };
    window.addEventListener('pointerup', onWindowPointerUp, true);
    tick().then(() => focusEntry(0));
    return () => {
      window.removeEventListener('pointerup', onWindowPointerUp, true);
    };
  });

  // Keep the Android Back button closing the menu before leaving the app (phone layouts only).
  $effect(() => {
    if (!contextMenu.isOpen) return;
    return pushBack(() => {
      if (viewport.isPhone && showFullSheet && contextMenu.options?.phoneBar) {
        showFullSheet = false;
      } else {
        contextMenu.close();
      }
    });
  });

  function onScrimClick() {
    if (Date.now() - openedAt < 200 && !pointerupAfterOpen) return;
    contextMenu.close();
  }

  // Dismissal while open: Escape and Tab from the keyboard; outside press, scroll, blur, resize.
  // Pointer and scroll inside any panel (root or flyout) do not dismiss.
  $effect(() => {
    if (!contextMenu.isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        if (levels.length > 0) closeLevel(levels.length);
        else contextMenu.dismiss();
      } else if (e.key === 'Tab') {
        contextMenu.close();
      }
    };
    const onPointerDown = (e: PointerEvent) => {
      // The phone scrim closes on click, not pointerdown, so the tap cannot fall through to the content beneath.
      if (e.target === scrimEl) return;
      if (e.target instanceof Element && e.target.closest('[data-context-menu]')) return;
      contextMenu.close();
    };
    const onScroll = (e: Event) => {
      if (e.target instanceof Element && e.target.closest('[data-context-menu]')) return;
      contextMenu.close();
    };
    const onClose = () => contextMenu.close();
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('blur', onClose);
    window.addEventListener('resize', onClose);
    return () => {
      window.removeEventListener('keydown', onKey, true);
      window.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('blur', onClose);
      window.removeEventListener('resize', onClose);
    };
  });

  $effect(() => () => {
    cancelHover();
    if (typedTimer !== null) window.clearTimeout(typedTimer);
    trimLevels(0);
  });
</script>

{#if contextMenu.isOpen}
  {#if viewport.isPhone}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="sheet-scrim"
      class:spotlight-scrim={!!contextMenu.options?.spotlight}
      bind:this={scrimEl}
      use:portal
      onclick={onScrimClick}
    ></div>
    {#if contextMenu.options?.spotlight}
      {@const spot = contextMenu.options?.spotlight ?? { x: 0, y: 0, width: 0, height: 0 }}
      <div
        class="spotlight"
        data-context-menu
        use:portal
        style:left="{spot.x}px"
        style:top="{spot.y}px"
        style:width="{spot.width}px"
        style:height="{spot.height}px"
      ></div>
    {/if}
  {/if}

  {#if !viewport.isPhone || !contextMenu.options?.phoneBar || showFullSheet}
    {#each renderedLevels as depth (depth)}
      {@const f = depth > 0 ? levels[depth - 1] : undefined}
      {@const search = f?.sub.search}
      {@const view = panelView(depth)}
      <div
        class="context-menu"
        class:sheet={viewport.isPhone}
        class:drilled={viewport.isPhone && depth > 0}
        class:flyout={!viewport.isPhone && depth > 0}
        role="menu"
        tabindex="-1"
        aria-label={f ? f.label : (contextMenu.options?.ariaLabel ?? 'Actions')}
        data-context-menu
        style:max-height={!viewport.isPhone && f
          ? `min(${f.sub.maxHeight ?? FLYOUT_MAX_HEIGHT}px, calc(100dvh - 16px))`
          : undefined}
        use:portal
        use:placeLevel={{ depth, rows: view.rows }}
        onpointerenter={cancelHover}
      >
        {#if viewport.isPhone && f}
          <div
            class="item back"
            role="menuitem"
            tabindex="-1"
            onclick={() => closeLevel(depth)}
            onpointermove={focusOnMove}
            onkeydown={(e) => onChromeKeydown(e, depth, () => closeLevel(depth))}
          >
            <span class="icon"><Icon icon={ChevronLeft} size={20} /></span>
            <span class="label">{f.label}</span>
          </div>
          <div class="separator" role="separator"></div>
        {/if}

        {#if search}
          <div class="search" role="none">
            <TextField
              placeholder={search.placeholder}
              aria-label={search.placeholder}
              autocomplete="off"
              value={f?.query ?? ''}
              oninput={(e) => onSearchInput(depth, e.currentTarget.value)}
              onkeydown={(e) => onSearchKeydown(e, depth)}
            />
          </div>
        {/if}

        {#if view.rows.length === 0}
          <div class="empty" role="none">{view.query ? 'No matches' : 'Nothing to show'}</div>
        {/if}

        {#each view.rows as item, i (i)}
          {#if isSeparator(item)}
            <div class="separator" role="separator"></div>
          {:else}
            <div
              class="item row"
              class:danger={item.danger}
              class:disabled={item.disabled}
              class:selected={item.selected}
              role={item.selected !== undefined ? 'menuitemradio' : 'menuitem'}
              tabindex="-1"
              aria-checked={item.selected !== undefined ? (item.selected ? 'true' : 'false') : undefined}
              aria-disabled={item.disabled ? 'true' : undefined}
              aria-keyshortcuts={item.shortcut}
              aria-haspopup={isSubmenuAction(item) ? 'menu' : undefined}
              aria-expanded={isSubmenuAction(item)
                ? (levels[depth]?.parentIndex === i ? 'true' : 'false')
                : undefined}
              onclick={(e) => clickRow(e, depth, i, item)}
              onpointerenter={(e) => enterRow(e, depth, i, item)}
              onpointermove={focusOnMove}
              onkeydown={(e) => onRowKeydown(e, depth, i, item)}
            >
              {#if item.icon}
                <span class="icon"><Icon icon={item.icon} size={viewport.isPhone ? 20 : 16} /></span>
              {/if}
              <span class="label">{item.label}</span>
              {#if isSubmenuAction(item)}
                <span class="chevron"><Icon icon={ChevronRight} size={16} /></span>
              {:else if item.shortcut}
                <kbd class="shortcut">{item.shortcut}</kbd>
              {:else if item.selected}
                <span class="check"><Icon icon={Check} size={viewport.isPhone ? 20 : 16} /></span>
              {/if}
            </div>
          {/if}
        {/each}

        {#if viewport.isPhone}
          <div class="separator" role="separator"></div>
          <div
            class="item cancel"
            role="menuitem"
            tabindex="-1"
            onclick={() => selectByPointer(CANCEL)}
            onpointermove={focusOnMove}
            onkeydown={(e) => onChromeKeydown(e, depth, () => selectByPointer(CANCEL))}
          >
            <span class="label">Cancel</span>
          </div>
        {/if}
      </div>
    {/each}
  {/if}

  {#if viewport.isPhone && contextMenu.options?.phoneBar && !showFullSheet}
    <div
      class="phone-action-bar"
      bind:this={bottomBarEl}
      role="group"
      aria-label="Message actions"
      data-context-menu
      use:portal
    >
      {#each contextMenu.options?.phoneBar ?? [] as item, i (i)}
        {#if !isSeparator(item)}
          <button
            type="button"
            class="bar-button"
            class:danger={item.danger}
            disabled={item.disabled}
            onclick={() => selectByPointer(item)}
          >
            {#if item.icon}
              <span class="icon"><Icon icon={item.icon} size={24} /></span>
            {/if}
            <span class="label">{item.label}</span>
          </button>
        {/if}
      {/each}
      <button
        type="button"
        class="bar-button"
        aria-label="More"
        onclick={() => {
          showFullSheet = true;
        }}
      >
        <span class="icon"><Icon icon={Ellipsis} size={24} /></span>
        <span class="label">More</span>
      </button>
    </div>
  {/if}

  {#if viewport.isPhone && levels.length === 0 && contextMenu.options?.reactions}
    {@const reactions = contextMenu.options?.reactions ?? NO_REACTIONS}
    <div class="reaction-float" role="none" data-context-menu use:portal use:placeFloat={reactions.anchor}>
      <QuickReactions
        emojis={reactions.emojis}
        active={reactions.active}
        size="touch"
        onpick={(emoji) => {
          // Read the handler first: closing clears the options, and `reactions` follows them.
          const pick = reactions.onpick;
          contextMenu.close();
          pick(emoji);
        }}
        onmore={() => {
          const more = reactions.onmore;
          contextMenu.close();
          more();
        }}
      />
    </div>
  {/if}

  {#if !viewport.isPhone && contextMenu.options?.reactions}
    {@const reactions = contextMenu.options?.reactions ?? NO_REACTIONS}
    <div
      class="desktop-reaction-strip"
      role="toolbar"
      aria-label="Quick reactions"
      data-context-menu
      use:portal
      use:placeDesktopStrip
    >
      <QuickReactions
        emojis={reactions.emojis}
        active={reactions.active}
        size="compact"
        onpick={(emoji) => {
          const pick = reactions.onpick;
          contextMenu.close();
          pick(emoji);
        }}
        onmore={() => {
          const more = reactions.onmore;
          contextMenu.close();
          more();
        }}
      />
    </div>
  {/if}
{/if}

<style>
  .context-menu {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 110;
    min-width: 180px;
    max-width: 280px;
    padding: 5px;
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-2xl);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter-strong);
    backdrop-filter: var(--alloy-glass-filter-strong);
    box-shadow: var(--alloy-glass-shadow), var(--alloy-glass-highlight);
    color: var(--alloy-color-text-primary);
    font-family: var(--alloy-typography-font-family-sans);
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-regular);
    line-height: var(--alloy-typography-line-height-normal);
  }

  /* Submenu flyout: scrolls past its max height; the scrollbar sits on the surface, not a transparent track. */
  .context-menu.flyout {
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-color: var(--alloy-color-border-prominent) var(--alloy-color-surface-elevated);
  }

  .search {
    padding: 0 0 var(--alloy-spacing-scale-1);
  }
  .search :global(.text-field) {
    width: 100%;
    box-sizing: border-box;
  }

  .empty {
    padding: var(--alloy-spacing-scale-2);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-sm);
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    height: 34px;
    padding: 0 10px;
    border-radius: var(--alloy-radii-xl);
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
    outline: none;
  }

  .item:focus {
    background: var(--alloy-glass-control-hover);
  }

  .item.danger {
    color: var(--alloy-color-status-error);
  }

  .item.disabled {
    color: var(--alloy-color-text-muted);
    cursor: default;
  }

  .check {
    display: inline-flex;
    flex: none;
    align-items: center;
    color: var(--alloy-color-accent-primary);
  }

  .chevron {
    display: inline-flex;
    flex: none;
    align-items: center;
    color: var(--alloy-color-text-muted);
  }

  .item.selected {
    font-weight: var(--alloy-typography-weight-medium);
  }
  .icon {
    display: inline-flex;
    flex: none;
  }

  .label {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .shortcut {
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }

  .separator {
    height: 1px;
    margin: var(--alloy-spacing-scale-1) 0;
    background: var(--alloy-color-border-subtle);
  }


  /* Phone: the quick reactions float beside the pressed message as a pill, above the action sheet. */
  .reaction-float {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 111;
    display: flex;
    max-width: calc(100vw - 2 * var(--alloy-spacing-scale-2));
    padding: var(--alloy-spacing-scale-1);
    overflow-x: auto;
    scrollbar-width: none;
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-elevated);
    box-shadow: var(--alloy-elevation-lg);
    animation: alloy-fade-in var(--alloy-motion-duration-normal) ease-out backwards;
  }
  .reaction-float::-webkit-scrollbar {
    display: none;
  }

  /* Phone: the menu is a bottom action sheet. Rows are 44px for touch; the Cancel row closes it. */
  .context-menu.sheet {
    top: auto;
    left: 0;
    right: 0;
    bottom: 0;
    min-width: 0;
    max-width: none;
    max-height: 90dvh;
    overflow-y: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2)
      calc(var(--alloy-spacing-scale-2) + var(--alloy-safe-area-bottom));
    border: 0;
    border-radius: var(--alloy-radii-lg) var(--alloy-radii-lg) 0 0;
    box-shadow: var(--alloy-elevation-lg);
    animation: alloy-action-sheet-up var(--alloy-motion-duration-normal) ease-out backwards;
  }
  /* A drilled submenu replaces the sheet's content in place rather than sliding up again. */
  .context-menu.sheet.drilled {
    animation: alloy-fade-in var(--alloy-motion-duration-normal) ease-out backwards;
  }
  .sheet .item {
    height: 44px;
    padding: 0 var(--alloy-spacing-scale-4);
    font-size: var(--alloy-typography-size-lg);
    border-radius: var(--alloy-radii-md);
  }
  .sheet .shortcut {
    display: none;
  }
  .sheet .item.back .label {
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .sheet .item.cancel {
    justify-content: center;
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .sheet .item.cancel .label {
    flex: none;
  }

  /* Phone: scrim behind the action sheet. Tapping it closes the sheet (click, not pointerdown). */
  .sheet-scrim {
    position: fixed;
    inset: 0;
    z-index: 109;
    background: var(--alloy-color-scrim);
    animation: alloy-fade-in var(--alloy-motion-duration-normal) ease-out backwards;
  }
  .sheet-scrim.spotlight-scrim {
    background: transparent;
  }

  .spotlight {
    position: fixed;
    z-index: 109;
    border-radius: var(--alloy-radii-md);
    box-shadow: 0 0 0 9999px var(--alloy-color-scrim);
    pointer-events: none;
  }

  .phone-action-bar {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 110;
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4)
      calc(var(--alloy-spacing-scale-3) + var(--alloy-safe-area-bottom));
    background: var(--alloy-color-surface-panel);
    border-top: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg) var(--alloy-radii-lg) 0 0;
    box-shadow: var(--alloy-elevation-lg);
    animation: alloy-action-sheet-up var(--alloy-motion-duration-normal) ease-out backwards;
  }
  .bar-button {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    min-height: 48px;
    padding: var(--alloy-spacing-scale-1) 0;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-text-primary);
    font-family: var(--alloy-typography-font-family-sans);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-medium);
    cursor: pointer;
    outline: none;
    user-select: none;
  }
  .bar-button:active,
  .bar-button:focus-visible {
    background: var(--alloy-color-surface-hover);
  }
  .bar-button.danger {
    color: var(--alloy-color-status-error);
  }
  .bar-button .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--alloy-color-accent-primary);
  }

  .desktop-reaction-strip {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 110;
    display: inline-flex;
    align-items: center;
    padding: var(--alloy-spacing-scale-1);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-elevated);
    box-shadow: var(--alloy-elevation-lg);
    animation: alloy-fade-in var(--alloy-motion-duration-normal) ease-out backwards;
  }


  @keyframes alloy-fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes alloy-action-sheet-up {
    from {
      transform: translateY(100%);
    }
    to {
      transform: translateY(0);
    }
  }
</style>
