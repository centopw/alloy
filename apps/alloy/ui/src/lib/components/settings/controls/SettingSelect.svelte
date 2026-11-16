<script lang="ts">
  import { Select, type SelectOption } from '$lib/components/common';
  import SettingRow from './SettingRow.svelte';

  interface Props {
    label: string;
    description?: string;
    value: string;
    options: SelectOption[];
    onchange: (value: string) => void;
    disabled?: boolean;
  }

  let { label, description, value, options, onchange, disabled = false }: Props = $props();

  const selectId = $props.id();
</script>

<SettingRow {label} {description} forId={selectId}>
  <Select
    id={selectId}
    {label}
    {value}
    {options}
    {onchange}
    {disabled}
    class="select-control"
  />
</SettingRow>

<style>
  :global(.select-control) {
    flex-shrink: 0;
    min-width: 160px;
    max-width: 55%;
  }
</style>
