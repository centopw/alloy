<script lang="ts">
  interface Column {
    value: string;
    label: string;
  }

  interface Row {
    id: string;
    label: string;
    value: string;
  }

  interface Props {
    legend: string;
    columns: Column[];
    rows: Row[];
    onchange: (rowId: string, value: string) => void;
  }

  let { legend, columns, rows, onchange }: Props = $props();

  const groupName = $props.id();
</script>

<fieldset class="matrix">
  <legend class="legend">{legend}</legend>
  <table>
    <thead>
      <tr>
        <th scope="col" class="row-head"><span class="sr-only">Item</span></th>
        {#each columns as column (column.value)}
          <th scope="col" class="col-head">{column.label}</th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as row (row.id)}
        <tr>
          <th scope="row" class="row-head">{row.label}</th>
          {#each columns as column (column.value)}
            <td class="cell">
              <input
                type="radio"
                name={`${groupName}-${row.id}`}
                aria-label={`${row.label}: ${column.label}`}
                value={column.value}
                checked={row.value === column.value}
                onchange={() => onchange(row.id, column.value)}
              />
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</fieldset>

<style>
  .matrix {
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

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--alloy-typography-size-sm);
  }

  th,
  td {
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border-top: 1px solid var(--alloy-color-border-subtle);
    text-align: left;
  }

  .col-head,
  .cell {
    text-align: center;
  }

  .col-head {
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-secondary);
  }

  .row-head {
    font-weight: var(--alloy-typography-weight-regular);
    color: var(--alloy-color-text-primary);
  }

  .cell input {
    accent-color: var(--alloy-color-accent-primary);
    cursor: pointer;
  }

  .cell input:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }

  .sr-only {
    position: absolute;
    width: 0;
    height: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>
