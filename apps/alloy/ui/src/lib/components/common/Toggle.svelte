<script lang="ts">
  /**
   * Alloy switch. Behaviour (role, aria-checked, Space/Enter, focus) comes from Bits UI via the
   * vendored shadcn-svelte Switch; this wrapper keeps the props API every call site already uses.
   * ADR 0005's 44px touch target is restored in mobile.css, keyed on `[data-slot='switch']`, so the
   * visible track keeps the primitive's size. See decision 0012.
   */
  import { Switch as SwitchPrimitive } from '$lib/components/ui/switch/index.js';
  import { cn } from '$lib/utils.js';

  interface Props {
    checked?: boolean;
    label: string;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
    class?: string;
  }

  let {
    checked = $bindable(false),
    label,
    disabled = false,
    onchange,
    class: className = ''
  }: Props = $props();
</script>

<SwitchPrimitive
  bind:checked
  {disabled}
  aria-label={label}
  onCheckedChange={(next: boolean) => onchange?.(next)}
  class={cn('alloy-toggle', className)}
/>
