<script lang="ts">
  import { SettingGroup } from '$lib/components/settings/controls';
  import { modKey } from '$lib/platform/shortcuts';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { capabilities, currentEnv } from '$lib/platform/capabilities';

  // A chord is a list of keys pressed together. Each row lists alternative chords.
  type Chord = string[];
  type Row = { action: string; chords: Chord[] };
  type Group = { title: string; rows: Row[] };

  const mod = modKey();
  const enterToSend = $derived(prefs.enterToSend);
  const caps = $derived(capabilities(currentEnv(viewport.isPhone)));
  const groups: Group[] = $derived([
    {
      title: 'Global',
      rows: [
        { action: 'Jump to a conversation or command', chords: [[mod, 'K']] },
        { action: 'Open settings', chords: [[mod, ',']] },
        { action: 'Refresh (restart sync, reload the open conversation)', chords: [[mod, 'R'], ['F5']] },
        ...(caps.keyboardShortcuts ? [{ action: 'Search in the open conversation', chords: [[mod, 'F']] }] : []),
      ],
    },
    {
      title: 'Composer',
      rows: [
        {
          action: 'Send message',
          chords: enterToSend ? [['Enter']] : [[mod, 'Enter']],
        },
        {
          action: 'Insert a new line',
          chords: enterToSend
            ? [['Shift', 'Enter'], [mod, 'Enter']]
            : [['Enter'], ['Shift', 'Enter']],
        },
        { action: 'Pick the highlighted mention', chords: [['Tab'], ['Enter']] },
        { action: 'Move through mention suggestions', chords: [['↑'], ['↓']] },
        { action: 'Edit your last message (empty field)', chords: [['↑']] },
        { action: 'Cancel reply or edit', chords: [['Esc']] },
      ],
    },
    {
      title: 'Dialogs and menus',
      rows: [
        { action: 'Close a dialog, menu, palette or popover', chords: [['Esc']] },
        { action: 'Move focus within an open dialog', chords: [['Tab'], ['Shift', 'Tab']] },
        { action: 'Move through menus and palette results', chords: [['↑'], ['↓']] },
        { action: 'Open the context menu for the focused item', chords: [['Shift', 'F10'], ['Menu']] },
        { action: 'Move between tabs and segmented options', chords: [['←'], ['→']] },
        { action: 'Turn a switch on or off', chords: [['Space'], ['Enter']] },
      ],
    },
    ...(caps.resizablePanels
      ? [
          {
            title: 'Layout',
            rows: [
              { action: 'Resize a focused panel divider', chords: [['←'], ['→']] },
              { action: 'Resize a focused panel divider in larger steps', chords: [['Shift', '←'], ['Shift', '→']] },
              { action: 'Set a focused panel divider to its minimum or maximum width', chords: [['Home'], ['End']] },
            ],
          },
        ]
      : []),
  ]);
</script>

{#snippet chordList(chords: Chord[])}
  {#each chords as chord, i (i)}
    <!-- "or" belongs to the chord that follows it, so a wrapped row keeps the pair together. -->
    <span class="alt">
      {#if i > 0}
        <span class="or">or</span>
      {/if}
      <span class="chord">
        {#each chord as key, j (j)}
          {#if j > 0}
            <span class="plus" aria-hidden="true">+</span>
          {/if}
          <kbd>{key}</kbd>
        {/each}
      </span>
    </span>
  {/each}
{/snippet}

<div class="section">
  {#each groups as group (group.title)}
    <SettingGroup title={group.title}>
      <table class="shortcuts">
        <thead class="visually-hidden">
          <tr>
            <th scope="col">Action</th>
            <th scope="col">Keys</th>
          </tr>
        </thead>
        <tbody>
          {#each group.rows as row (row.action)}
            <tr>
              <th scope="row" class="action">{row.action}</th>
              <td class="keys">{@render chordList(row.chords)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </SettingGroup>
  {/each}
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }

  .shortcuts {
    width: 100%;
    border-collapse: collapse;
  }

  .shortcuts tr + tr th,
  .shortcuts tr + tr td {
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .action,
  .keys {
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    text-align: left;
    vertical-align: middle;
  }

  .action {
    width: 60%;
    font-weight: var(--alloy-typography-weight-medium);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-primary);
  }

  .keys {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
    /* Keys are the control in this row: every other settings row puts its control on the right
       rail, so shortcuts do too, wrapping as whole alternatives instead of breaking mid-chord. */
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    text-align: right;
  }

  .alt {
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }

  .chord {
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    white-space: nowrap;
  }

  .or {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }

  .plus {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }

  kbd {
    display: inline-block;
    min-width: 1.5rem;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-surface-subtle);
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-typography-size-xs);
    line-height: 1.4;
    text-align: center;
    color: var(--alloy-color-text-primary);
  }

  /* Phone: each shortcut is a stacked 56px row (action above keys) instead of a three-column table. */
  @media (max-width: 899px) {
    .shortcuts,
    .shortcuts tbody {
      display: block;
    }
    .shortcuts tr {
      display: flex;
      flex-direction: column;
      gap: var(--alloy-spacing-scale-1);
      box-sizing: border-box;
      min-height: 56px;
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    }
    .shortcuts tr + tr {
      border-top: 1px solid var(--alloy-color-border-subtle);
    }
    .shortcuts tr + tr th,
    .shortcuts tr + tr td {
      border-top: none;
    }
    .action,
    .keys {
      display: block;
      width: auto;
      padding: 0;
    }
    .action {
      font-size: var(--alloy-typography-size-lg);
    }
    .keys {
      font-size: var(--alloy-typography-size-sm);
      text-align: left;
    }
    .alt + .alt {
      margin-left: var(--alloy-spacing-scale-2);
    }
  }
</style>
