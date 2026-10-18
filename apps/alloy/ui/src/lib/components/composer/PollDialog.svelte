<script lang="ts">
  import { Modal, Button, Toggle } from '../common';
  import { timelineStore } from '$lib/stores/timeline.svelte';

  const MIN_ANSWERS = 2;
  const MAX_ANSWERS = 10;

  interface Props {
    open: boolean;
    onclose: () => void;
  }
  let { open, onclose }: Props = $props();

  let question = $state('');
  let answers = $state<string[]>(['', '']);
  let multiple = $state(false);
  let disclosed = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (open) {
      question = '';
      answers = ['', ''];
      multiple = false;
      disclosed = false;
      busy = false;
      error = null;
    }
  });

  const valid = $derived(
    question.trim().length > 0 &&
      answers.length >= MIN_ANSWERS &&
      answers.length <= MAX_ANSWERS &&
      answers.every((a) => a.trim().length > 0),
  );

  function addAnswer() {
    if (answers.length < MAX_ANSWERS) answers = [...answers, ''];
  }

  function removeAnswer(index: number) {
    if (answers.length > MIN_ANSWERS) answers = answers.filter((_, i) => i !== index);
  }

  async function submit(event: Event) {
    event.preventDefault();
    if (!valid || busy) return;
    busy = true;
    error = null;
    const failure = await timelineStore.sendPoll(
      question.trim(),
      answers.map((a) => a.trim()),
      disclosed,
      multiple ? answers.length : 1,
    );
    busy = false;
    if (failure === null) {
      onclose();
    } else {
      error = failure;
    }
  }
</script>

<Modal {open} title="Create poll" {onclose} width="440px">
  <form class="form" onsubmit={submit}>
    <label class="field">
      <span>Question</span>
      <input type="text" bind:value={question} maxlength="300" autocomplete="off" />
    </label>

    <fieldset class="answers">
      <legend>Answers</legend>
      {#each answers as _, i (i)}
        <div class="answer-row">
          <input
            type="text"
            aria-label="Answer {i + 1}"
            bind:value={answers[i]}
            maxlength="200"
            autocomplete="off"
          />
          <Button
            variant="ghost"
            size="sm"
            ariaLabel="Remove answer {i + 1}"
            disabled={answers.length <= MIN_ANSWERS}
            onclick={() => removeAnswer(i)}
          >
            Remove
          </Button>
        </div>
      {/each}
      <div>
        <Button variant="ghost" size="sm" disabled={answers.length >= MAX_ANSWERS} onclick={addAnswer}>
          Add answer
        </Button>
      </div>
    </fieldset>

    <div class="row">
      <span>Allow multiple answers</span>
      <Toggle label="Allow multiple answers" checked={multiple} onchange={(v) => (multiple = v)} />
    </div>
    <div class="row">
      <span>Show results while voting</span>
      <Toggle label="Show results while voting" checked={disclosed} onchange={(v) => (disclosed = v)} />
    </div>

    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="actions">
      <Button variant="ghost" onclick={onclose}>Cancel</Button>
      <Button type="submit" disabled={busy || !valid}>{busy ? 'Creating...' : 'Create poll'}</Button>
    </div>
  </form>
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  input {
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-primary);
    background: var(--alloy-color-surface-subtle);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
  }
  input:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }
  .answers {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    margin: 0;
    padding: 0;
    border: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .answers legend {
    margin-bottom: var(--alloy-spacing-scale-1);
  }
  .answer-row {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }
  .answer-row input {
    flex: 1;
    min-width: 0;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-3);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-primary);
  }
  .error {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-status-error);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--alloy-spacing-scale-2);
  }
</style>
