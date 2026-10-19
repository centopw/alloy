<script lang="ts">
  import Button from '$lib/components/common/Button.svelte';
  import Modal from '$lib/components/common/Modal.svelte';
  import { parseReleaseNotes, type ReleaseNoteInline } from '$lib/content/releaseNotes';
  import { openExternal } from '$lib/platform/links';
  import { updatesStore } from '$lib/stores/updates.svelte';

  const info = $derived(updatesStore.info);
  const blocks = $derived(parseReleaseNotes(info?.notes ?? ''));
  const title = $derived(info ? `What's new in Alloy ${info.version}` : "What's new");

  const close = () => updatesStore.closeChangelog();
</script>

{#snippet runs(list: ReleaseNoteInline[])}
  {#each list as run, i (i)}
    {#if run.kind === 'strong'}
      <strong>{run.text}</strong>
    {:else if run.kind === 'code'}
      <code>{run.text}</code>
    {:else if run.kind === 'link'}
      <a
        href={run.href}
        onclick={(e) => {
          e.preventDefault();
          void openExternal(run.href);
        }}>{run.text}</a
      >
    {:else}
      {run.text}
    {/if}
  {/each}
{/snippet}

<Modal open={updatesStore.changelogOpen} {title} onclose={close}>
  <div class="notes">
    {#if blocks.length === 0}
      <p class="empty">No release notes were published for this version.</p>
    {:else}
      {#each blocks as block, i (i)}
        {#if block.kind === 'heading'}
          <svelte:element this={block.level === 2 ? 'h3' : 'h4'} class="heading">
            {@render runs(block.inlines)}
          </svelte:element>
        {:else if block.kind === 'paragraph'}
          <p>{@render runs(block.inlines)}</p>
        {:else if block.kind === 'list'}
          <ul>
            {#each block.items as item, j (j)}
              <li>{@render runs(item)}</li>
            {/each}
          </ul>
        {:else if block.kind === 'code'}
          <pre><code>{block.text}</code></pre>
        {:else}
          <hr />
        {/if}
      {/each}
    {/if}
  </div>

  <footer class="actions">
    <Button variant="ghost" onclick={() => updatesStore.skipVersion()}>Skip this version</Button>
    <div class="spacer"></div>
    <Button variant="ghost" onclick={() => void updatesStore.openReleasePage()}>View release page</Button>
    <Button variant="ghost" onclick={close}>Later</Button>
    <Button
      disabled={updatesStore.busy}
      onclick={() => void updatesStore.primary()}>{updatesStore.primaryLabel}</Button
    >
  </footer>
</Modal>

<style>
  .notes {
    max-height: 60vh;
    overflow-y: auto;
    padding: var(--alloy-spacing-scale-4);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-base);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .heading {
    margin: var(--alloy-spacing-scale-3) 0 var(--alloy-spacing-scale-2);
    color: var(--alloy-color-text-primary);
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
  }

  p {
    margin: 0 0 var(--alloy-spacing-scale-3);
  }

  ul {
    margin: 0 0 var(--alloy-spacing-scale-3);
    padding-left: var(--alloy-spacing-scale-5);
  }

  li {
    margin-bottom: var(--alloy-spacing-scale-1);
  }

  pre {
    margin: 0 0 var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3);
    overflow-x: auto;
    border: 1px solid var(--alloy-color-border-subtle);
    font-size: var(--alloy-typography-size-sm);
    white-space: pre;
  }

  code {
    font-family: var(--alloy-font-mono);
  }

  a {
    color: var(--alloy-color-text-link);
  }

  hr {
    margin: var(--alloy-spacing-scale-4) 0;
    border: 0;
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .empty {
    color: var(--alloy-color-text-muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .spacer {
    flex: 1 1 auto;
  }
</style>
