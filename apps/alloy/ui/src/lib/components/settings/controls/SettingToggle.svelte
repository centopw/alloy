<script lang="ts">
  import Toggle from '../../common/Toggle.svelte';
  import SettingRow from './SettingRow.svelte';

  interface Props {
    label: string;
    description?: string;
    checked: boolean;
    onchange: (value: boolean) => void;
    disabled?: boolean;
    disabledReason?: string;
  }

  let { label, description, checked, onchange, disabled = false, disabledReason }: Props = $props();
</script>

<SettingRow {label} {description} reason={disabled ? disabledReason : undefined}>
  <span class="toggle-hit">
    <Toggle {checked} {label} {disabled} {onchange} />
  </span>
</SettingRow>

<style>
  .toggle-hit {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
  }

  @media (max-width: 899px) {
    .toggle-hit :global(.alloy-toggle) {
      position: relative;
      flex-shrink: 0;
    }
    /*
     * The shadcn-svelte track is 32x18.4px and carries a 1px border each side, so its padding box is
     * 16.4px tall and 30px wide. A pseudo-element resolves against that padding box: -14px
     * vertically gives 16.4 + 28 = 44.4px and -7px horizontally gives 30 + 14 = 44px, so the tap
     * target clears ADR 0005's 44x44 minimum without changing the visible track.
     */
    .toggle-hit :global(.alloy-toggle)::before {
      content: '';
      position: absolute;
      inset: -14px -7px;
    }
  }
</style>
