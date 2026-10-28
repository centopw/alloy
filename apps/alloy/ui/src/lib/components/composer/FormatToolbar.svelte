<script lang="ts">
  import Icon from '$lib/icons/Icon.svelte';
  import { Bold, Code, EyeOff, Italic, Link, Quote, Strikethrough } from '$lib/icons';
  import { shortcutLabel } from '$lib/platform/shortcuts';
  import type { WrapKind } from '$lib/content/markdownWrap';

  interface Props {
    onwrap: (kind: WrapKind) => void;
  }
  let { onwrap }: Props = $props();

  const tools: { kind: WrapKind; label: string; icon: typeof Bold; keys?: string }[] = [
    { kind: 'bold', label: 'Bold', icon: Bold, keys: 'B' },
    { kind: 'italic', label: 'Italic', icon: Italic, keys: 'I' },
    { kind: 'strike', label: 'Strikethrough', icon: Strikethrough, keys: 'Shift+X' },
    { kind: 'code', label: 'Code', icon: Code, keys: 'Shift+M' },
    { kind: 'spoiler', label: 'Spoiler', icon: EyeOff, keys: 'Shift+P' },
    { kind: 'quote', label: 'Quote', icon: Quote, keys: 'Shift+Q' },
    { kind: 'link', label: 'Link', icon: Link, keys: 'Shift+L' },
  ];
</script>

<div class="toolbar" role="toolbar" aria-label="Text formatting">
  {#each tools as tool (tool.kind)}
    <!-- The press must not take focus, or the textarea would lose its selection. -->
    <button
      type="button"
      aria-label={tool.label}
      data-tooltip={tool.keys ? `${tool.label} (${shortcutLabel(tool.keys)})` : tool.label}
      onpointerdown={(e) => e.preventDefault()}
      onclick={() => onwrap(tool.kind)}
    >
      <Icon icon={tool.icon} size={16} />
    </button>
  {/each}
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-xl);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter);
    backdrop-filter: var(--alloy-glass-filter);
    box-shadow: var(--alloy-glass-shadow-soft), var(--alloy-glass-highlight);
  }
  button {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  @media (hover: hover) {
    button:hover {
      background: var(--alloy-glass-control-hover);
      color: var(--alloy-color-text-primary);
    }
  }
  button:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }
  @media (max-width: 899px) {
    .toolbar {
      overflow-x: auto;
      scrollbar-width: none;
    }
    button {
      flex: none;
      width: 44px;
      height: 44px;
    }
  }
</style>
