<script lang="ts">
  import Button from '$lib/components/common/Button.svelte';
  import Modal from '$lib/components/common/Modal.svelte';
  import { parseReleaseNotes, type ReleaseNoteInline } from '$lib/content/releaseNotes';
  import { openExternal } from '$lib/platform/links';
  import { updatesStore } from '$lib/stores/updates.svelte';

  const info = $derived(updatesStore.info);
  const blocks = $derived(parseReleaseNotes(info?.notes ?? ''));
  const title = $derived(info ? `What's new in Alloy ${info.version}` : "What's new");
  const percent = $derived(
    updatesStore.progress === null ? null : Math.round(updatesStore.progress * 100),
  );
  // Skip is an answer to "do you want this?"; once the download has started there is nothing to skip.
  const canSkip = $derived(Boolean(info?.available) && !info?.downloaded && !updatesStore.busy);

  const status = $derived.by(() => {
    switch (updatesStore.phase) {
      case 'error':
        return updatesStore.error ?? 'The update could not be completed.';
      case 'installing':
        return 'Installing update. Alloy will restart when it is done.';
      case 'downloading':
        return percent === null ? 'Downloading update...' : `Downloading update... ${percent}%`;
      case 'ready':
        return 'The update is downloaded. Restart Alloy to finish installing it.';
      default:
        if (!info?.available) return 'Alloy is up to date.';
        return info.can_install
          ? 'A new version is ready. Download it now, or keep using this version.'
          : 'This release is not available for in-app installation. Open the release page to download it.';
    }
  });

  const close = () => updatesStore.closeChangelog();

  function skip() {
    updatesStore.skipVersion();
    close();
  }
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

<Modal open={updatesStore.changelogOpen} {title} onclose={close} width="560px">
  <div class="changelog">
    <header class="summary">
      <div class="summary-text">
        <p class="version">{info?.version ? `Version ${info.version}` : 'Update'}</p>
        <p class="status" role="status">{status}</p>
      </div>
      {#if updatesStore.phase === 'downloading'}
        <div
          class="bar"
          class:indeterminate={percent === null}
          role="progressbar"
          aria-label="Download progress"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={percent ?? undefined}
        >
          <span class="bar-fill" style:width={percent === null ? '35%' : `${percent}%`}></span>
        </div>
      {/if}
    </header>

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
      {#if canSkip}
        <Button variant="ghost" onclick={skip}>Skip this version</Button>
      {/if}
      <span class="spacer"></span>
      {#if info?.can_install !== false}
        <Button variant="ghost" onclick={() => void updatesStore.openReleasePage()}>Release page</Button>
      {/if}
      <Button variant="ghost" onclick={close}>Later</Button>
      <Button disabled={updatesStore.busy} onclick={() => void updatesStore.primary()}>
        {updatesStore.primaryLabel}
      </Button>
    </footer>
  </div>
</Modal>

<style>
  .changelog {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
  }

  .summary {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }

  .version {
    margin: 0;
    font-size: var(--alloy-font-size-meta);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-letter-spacing-caps);
    text-transform: uppercase;
    color: var(--alloy-color-accent-primary);
  }

  .status {
    margin: 0;
    color: var(--alloy-color-text-secondary);
    line-height: var(--alloy-typography-line-height-normal);
  }

  .bar {
    position: relative;
    height: 4px;
    overflow: hidden;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-border-subtle);
  }

  .bar-fill {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--alloy-color-accent-primary);
    transition: width var(--alloy-motion-duration-fast) ease-out;
  }

  .bar.indeterminate .bar-fill {
    animation: bar-slide 1.2s ease-in-out infinite;
  }

  .notes {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-base);
    line-height: 1.55;
    overflow-wrap: anywhere;
  }

  .heading {
    margin: var(--alloy-spacing-scale-3) 0 var(--alloy-spacing-scale-2);
    color: var(--alloy-color-text-primary);
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
  }

  .heading:first-child {
    margin-top: 0;
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
    border-radius: var(--alloy-radii-md);
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
    margin: 0;
    color: var(--alloy-color-text-muted);
  }

  /* The actions stay reachable while long release notes scroll under them. */
  .actions {
    position: sticky;
    bottom: calc(-1 * var(--alloy-layout-gutter));
    z-index: 1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    margin: var(--alloy-spacing-scale-2) calc(-1 * var(--alloy-layout-gutter))
      calc(-1 * var(--alloy-layout-gutter));
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-gutter);
    border-top: 1px solid var(--alloy-glass-rim);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter-strong);
    backdrop-filter: var(--alloy-glass-filter-strong);
  }

  .spacer {
    flex: 1 1 auto;
  }

  @keyframes bar-slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(320%);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .bar.indeterminate .bar-fill {
      animation: none;
      width: 100% !important;
      opacity: 0.4;
    }
  }
</style>
