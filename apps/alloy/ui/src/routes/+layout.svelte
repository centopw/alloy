<script lang="ts">
  import "../lib/tokens.css";
  let { children } = $props();
</script>

{@render children()}

<style>
  :global(html, body) {
    margin: 0;
    padding: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background-color: var(--alloy-bg-base);
    color: var(--alloy-text-primary);
    font-family: var(--alloy-font-sans);
    -webkit-font-smoothing: antialiased;
  }

  :global(*, *::before, *::after) {
    box-sizing: border-box;
  }
</style>
