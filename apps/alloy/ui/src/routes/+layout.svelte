<script lang="ts">
  import "../lib/tokens.css";
  import "../lib/layout.css";
  import "../lib/mobile.css";
  import ProfileCardHost from '$lib/components/shell/ProfileCardHost.svelte';
  import { themeStore } from "$lib/theme/theme.svelte";
  import ContextMenu from "$lib/components/common/ContextMenu.svelte";
  import TooltipLayer from "$lib/components/common/TooltipLayer.svelte";

  let { children } = $props();
</script>

{@render children()}

<ContextMenu />
<TooltipLayer />
<ProfileCardHost />

<style>
  /* The document never scrolls: the page is pinned to the viewport and only inner regions scroll.
     Safe-area insets are applied by the shell itself (AdaptiveShell), not here. */
  :global(html, body) {
    position: fixed;
    inset: 0;
    margin: 0;
    padding: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    overscroll-behavior: none;
    box-sizing: border-box;
    background-color: var(--alloy-color-surface-canvas);
    color: var(--alloy-color-text-primary);
    font-family: var(--alloy-typography-font-family-sans);
    -webkit-font-smoothing: antialiased;
  }

  :global(*, *::before, *::after) {
    box-sizing: border-box;
  }
</style>
