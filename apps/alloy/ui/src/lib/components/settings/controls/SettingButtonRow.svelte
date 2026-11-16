<script lang="ts">
  import Button from '../../common/Button.svelte';
  import SettingRow from './SettingRow.svelte';

  interface Props {
    label: string;
    description?: string;
    buttonLabel: string;
    onclick: () => void;
    danger?: boolean;
    busy?: boolean;
  }

  let { label, description, buttonLabel, onclick, danger = false, busy = false }: Props = $props();
</script>

<SettingRow {label} {description} {busy}>
  <Button variant={danger ? 'danger' : 'ghost'} disabled={busy} {onclick} class="setting-button">
    {buttonLabel}
  </Button>
</SettingRow>

<style>
  :global(.setting-button) {
    flex-shrink: 0;
  }
</style>
