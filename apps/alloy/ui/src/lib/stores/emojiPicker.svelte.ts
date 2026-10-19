/** Anchor rectangle in viewport coordinates. A `DOMRect` satisfies this shape. */
export interface EmojiPickerAnchor {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface EmojiPickerOpenOptions {
  anchor: EmojiPickerAnchor;
  /** Called with the chosen emoji. The picker closes after this unless `keepOpen` is set. */
  onpick: (emoji: string) => void;
  keepOpen?: boolean;
  /** Called once when the picker closes for any reason. */
  onclose?: () => void;
}

class EmojiPickerStore {
  isOpen = $state(false);
  anchor = $state<EmojiPickerAnchor | null>(null);
  keepOpen = $state(false);
  #onpick: ((emoji: string) => void) | null = null;
  #onclose: (() => void) | null = null;

  open(options: EmojiPickerOpenOptions): void {
    const { x, y, width, height } = options.anchor;
    this.anchor = { x, y, width, height };
    this.keepOpen = options.keepOpen ?? false;
    this.#onpick = options.onpick;
    this.#onclose = options.onclose ?? null;
    this.isOpen = true;
  }

  close(): void {
    if (!this.isOpen) return;
    const onclose = this.#onclose;
    this.isOpen = false;
    this.anchor = null;
    this.#onpick = null;
    this.#onclose = null;
    onclose?.();
  }

  /** Delivers a pick from the content component to the requester. */
  pick(emoji: string): void {
    const onpick = this.#onpick;
    if (!this.keepOpen) this.close();
    onpick?.(emoji);
  }
}

export const emojiPicker = new EmojiPickerStore();
