<script lang="ts">
  interface Props {
    roomId: string;
    onSend: (text: string) => void;
  }

  let { roomId, onSend }: Props = $props();
  let text = $state("");

  function send() {
    const trimmed = text.trim();
    if (!trimmed) return;
    onSend(trimmed);
    text = "";
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      send();
    }
  }
</script>

<div class="composer" data-room-id={roomId}>
  <textarea
    class="composer-input"
    rows="1"
    placeholder="Write a message"
    aria-label="Message"
    bind:value={text}
    onkeydown={handleKeydown}
  ></textarea>
  <button class="composer-send" type="button" disabled={!text.trim()} onclick={send}>Send</button>
</div>

<style>
  .composer {
    display: flex;
    align-items: flex-end;
    gap: var(--alloy-spacing-2, 8px);
    margin: var(--alloy-spacing-3, 12px);
    padding: var(--alloy-spacing-2, 8px) var(--alloy-spacing-3, 12px);
    background: var(--alloy-color-surface-raised, var(--alloy-color-surface-canvas));
    border-radius: var(--alloy-radius-xl, 20px);
    border: 1px solid var(--alloy-color-border-subtle, transparent);
  }
  .composer:focus-within {
    border-color: var(--alloy-color-accent-default, currentColor);
  }
  .composer-input {
    flex: 1;
    resize: none;
    max-height: 160px;
    border: 0;
    outline: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    padding: var(--alloy-spacing-1, 4px) 0;
  }
  .composer-send {
    border: 0;
    background: var(--alloy-color-accent-default, currentColor);
    color: var(--alloy-color-accent-contrast, #fff);
    border-radius: var(--alloy-radius-lg, 14px);
    padding: var(--alloy-spacing-1, 4px) var(--alloy-spacing-3, 12px);
    font: inherit;
    cursor: pointer;
  }
  .composer-send:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
