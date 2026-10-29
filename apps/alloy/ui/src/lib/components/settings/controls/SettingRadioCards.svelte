<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Check } from '$lib/icons';

  interface Option {
    value: string;
    label: string;
    description?: string;
    preview?: Snippet;
  }

  interface Props {
    legend: string;
    value: string;
    options: Option[];
    onchange: (value: string) => void;
  }

  let { legend, value, options, onchange }: Props = $props();

  const groupName = $props.id();
</script>

<!-- Options are tiles, not cards: an even grid of fills with no border of their own, so the group
     plate stays the only outline on screen. Selection is a tint plus an inset accent ring, which
     cannot shift the layout and does not add another line. -->
<fieldset class="cards">
  <legend class="legend">{legend}</legend>
  <div class="grid">
    {#each options as option (option.value)}
      {@const selected = option.value === value}
      <label class="card" class:selected>
        <input
          class="sr-input"
          type="radio"
          name={groupName}
          value={option.value}
          checked={selected}
          onchange={() => onchange(option.value)}
        />
        {#if option.preview}
          <span class="preview" aria-hidden="true">{@render option.preview()}</span>
        {/if}
        <span class="footer">
          <span class="text">
            <span class="card-label">{option.label}</span>
            {#if option.description}
              <span class="card-description">{option.description}</span>
            {/if}
          </span>
          <span class="mark" class:on={selected} aria-hidden="true">
            {#if selected}<Icon icon={Check} size={16} />{/if}
          </span>
        </span>
      </label>
    {/each}
  </div>
</fieldset>

<style>
  .cards {
    border: none;
    margin: 0;
    padding: 0;
    min-width: 0;
    container-type: inline-size;
  }

  .legend {
    padding: 0;
    margin-bottom: var(--alloy-spacing-scale-2);
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }

  /* Equal tracks: three options fill the row instead of wrapping into a ragged 2 + 1 with a hole. */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(10rem, 1fr));
    gap: var(--alloy-spacing-scale-2);
  }

  /* The tile is the chat canvas, so a preview reads exactly as it will in the timeline (bubbles have
     contrast here without any frame of their own). Selection never repaints it either: a tint would
     wash out the very thing the tile exists to show, so it is only the accent ring and the check. */
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    padding: var(--alloy-spacing-scale-3);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-surface-canvas);
    cursor: pointer;
    transition:
      background-color var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      box-shadow var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease);
  }

  .card:hover {
    background: color-mix(in srgb, var(--alloy-color-surface-canvas) 94%, var(--alloy-color-text-primary));
  }

  .card.selected {
    box-shadow: inset 0 0 0 2px var(--alloy-color-accent-primary);
  }

  .card:has(input:focus-visible) {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }

  .sr-input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    margin: 0;
    pointer-events: none;
  }

  /* The preview is the tile's content, so it sits flush and draws no frame of its own. */
  .preview {
    display: flex;
    align-items: center;
    min-width: 0;
    min-height: 3.5rem;
  }

  .footer {
    display: flex;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-2);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }

  .card-label {
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }

  .card-description {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }

  /* The slot is always reserved, so labels keep the same width whether or not a tile is picked. */
  .mark {
    flex: none;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    margin-top: -1px;
    border-radius: var(--alloy-radii-full);
  }

  .mark.on {
    background: var(--alloy-color-accent-fill);
    color: var(--alloy-color-text-on-accent);
  }

  /* Below one tile's comfortable width, stack instead of leaving a half-empty row. */
  @container (max-width: 36rem) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
