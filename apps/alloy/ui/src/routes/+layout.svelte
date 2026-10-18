<script lang="ts">
  import "../lib/tokens.css";
  import "../lib/layout.css";
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
  :global(html, body) {
    margin: 0;
    padding: 0;
    width: 100%;
    height: 100%;
    padding-top: var(--alloy-safe-area-top);
    padding-bottom: var(--alloy-safe-area-bottom);
    padding-left: var(--alloy-safe-area-left);
    padding-right: var(--alloy-safe-area-right);
    background-color: var(--alloy-color-surface-canvas);
    color: var(--alloy-color-text-primary);
    font-family: var(--alloy-typography-font-family-sans);
    -webkit-font-smoothing: antialiased;
  }

  :global(*, *::before, *::after) {
    box-sizing: border-box;
  }
</style>
