<script lang="ts">
  import type { Snippet } from 'svelte';

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
        <span class="card-label">{option.label}</span>
        {#if option.description}
          <span class="card-description">{option.description}</span>
        {/if}
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
  }

  .legend {
    padding: 0;
    margin-bottom: var(--alloy-spacing-scale-2);
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }

  .grid {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-3);
  }

  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-3);
    min-width: max-content;
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-surface-subtle);
    cursor: pointer;
    transition: border-color var(--alloy-motion-duration-fast), box-shadow var(--alloy-motion-duration-fast);
  }

  .card:hover {
    background: var(--alloy-color-surface-hover);
  }

  .card.selected {
    border-color: var(--alloy-color-accent-primary);
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

  .preview {
    display: block;
    border-radius: var(--alloy-radii-md);
    overflow: hidden;
  }

  .card-label {
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }

  .card-description {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-secondary);
  }
  @media (max-width: 899px) {
    .grid {
      flex-direction: column;
    }
    .card {
      width: 100%;
      min-width: 0;
      min-height: 56px;
    }
    .card-label {
      font-size: var(--alloy-typography-size-base);
    }
  }
</style>
