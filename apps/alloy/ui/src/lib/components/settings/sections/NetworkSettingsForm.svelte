<script lang="ts">
  import { onMount } from 'svelte';
  import { Button, Select, TextField } from '$lib/components/common';
  import type { NetworkInfoDto, NetworkSettingDto } from '$lib/generated';
  import { errorCode, errorMessage } from '$lib/stores/errors';
  import { session } from '$lib/stores/session.svelte';

  import { t } from '$lib/i18n';

  interface Props {
    /** The network whose declared runtime settings this form edits. */
    info: NetworkInfoDto;
  }

  let { info }: Props = $props();

  // Two instances never share ids: settings and onboarding can both be mounted.
  const idPrefix = $props.id();
  const network = $derived(info.network);
  const settings = $derived(info.setup?.settings ?? []);

  // One draft per setting: text and secrets are typed, choices are picked, and the engine keeps
  // the value in effect. Nothing is stored until its own Save runs.
  let drafts = $state<Record<string, string>>({});
  let errors = $state<Record<string, string | null>>({});
  let busyKey = $state<string | null>(null);

  onMount(() => {
    for (const setting of settings) drafts[setting.key] = setting.value ?? '';
  });

  /** Known keys carry localized copy; anything the engine adds later still renders. */
  function labelOf(setting: NetworkSettingDto): string {
    switch (setting.key) {
      case 'proxy':
        return t('network-setting-proxy-label');
      case 'platform':
        return t('network-setting-platform-label');
      default:
        return setting.label;
    }
  }

  function hintOf(setting: NetworkSettingDto): string | null {
    switch (setting.key) {
      case 'proxy':
        return t('network-setting-proxy-hint');
      case 'platform':
        return t('network-setting-platform-hint');
      default:
        return null;
    }
  }

  function optionLabel(setting: NetworkSettingDto, value: string): string {
    if (setting.key === 'platform') {
      if (value === 'android') return t('network-setting-platform-android');
      if (value === 'ios') return t('network-setting-platform-ios');
    }
    return setting.options.find((option) => option.value === value)?.label ?? value;
  }

  /** What the person needs to know about the value now in effect, if anything. */
  function stateOf(setting: NetworkSettingDto): string | null {
    if (setting.kind === 'choice') return null;
    if (setting.kind === 'secret') {
      return setting.value === null
        ? t('network-setting-secret-unset')
        : t('network-setting-secret-hidden');
    }
    return setting.value === null ? t('network-setting-unset') : setting.value;
  }

  function draftOf(setting: NetworkSettingDto): string {
    return drafts[setting.key] ?? '';
  }

  function dirty(setting: NetworkSettingDto): boolean {
    return draftOf(setting) !== (setting.value ?? '');
  }

  /** An empty text setting is cleared with its Clear button, never with Save. */
  function saveable(setting: NetworkSettingDto): boolean {
    if (busyKey !== null || !dirty(setting)) return false;
    if (setting.kind === 'choice') return true;
    return draftOf(setting).trim() !== '';
  }

  /** The value the engine reports after a refresh; a gone setting falls back to empty. */
  function storedValue(key: string): string {
    const settings = session.networks.find((entry) => entry.network === network)?.setup?.settings;
    return settings?.find((entry) => entry.key === key)?.value ?? '';
  }

  async function save(setting: NetworkSettingDto) {
    if (!saveable(setting)) return;
    busyKey = setting.key;
    errors[setting.key] = null;
    try {
      await session.setNetworkSetting(network, setting.key, draftOf(setting).trim());
      // The engine's answer is the draft from now on, so the field never drifts from the connector.
      drafts[setting.key] = storedValue(setting.key);
    } catch (e) {
      errors[setting.key] =
        errorCode(e) === 'invalid_argument'
          ? setting.key === 'proxy'
            ? t('network-setting-proxy-invalid')
            : t('network-setting-invalid')
          : errorMessage(e);
    } finally {
      busyKey = null;
    }
  }

  async function clear(setting: NetworkSettingDto) {
    if (busyKey !== null) return;
    busyKey = setting.key;
    errors[setting.key] = null;
    try {
      await session.clearNetworkSetting(network, setting.key);
      drafts[setting.key] = storedValue(setting.key);
    } catch (e) {
      errors[setting.key] = errorMessage(e);
    } finally {
      busyKey = null;
    }
  }
</script>

<div class="network-settings">
  {#each settings as setting (setting.key)}
    <div class="setting">
      <label class="label" for="{idPrefix}-{setting.key}">{labelOf(setting)}</label>
      <div class="controls">
        {#if setting.kind === 'choice'}
          <Select
            id="{idPrefix}-{setting.key}"
            label={labelOf(setting)}
            value={draftOf(setting)}
            options={setting.options.map((option) => ({
              value: option.value,
              label: optionLabel(setting, option.value)
            }))}
            onchange={(value) => (drafts[setting.key] = value)}
          />
        {:else}
          <div class="field">
            <TextField
              id="{idPrefix}-{setting.key}"
              type={setting.kind === 'secret' ? 'password' : 'text'}
              value={draftOf(setting)}
              placeholder={setting.kind === 'secret' && setting.value === null
                ? t('network-setting-secret-placeholder')
                : undefined}
              autocomplete="off"
              spellcheck={false}
              invalid={errors[setting.key] !== null && errors[setting.key] !== undefined}
              oninput={(e) => (drafts[setting.key] = e.currentTarget.value)}
            />
          </div>
        {/if}
        <Button
          type="button"
          size="sm"
          disabled={!saveable(setting)}
          onclick={() => void save(setting)}
        >
          {busyKey === setting.key ? t('network-setting-saving') : t('common-save')}
        </Button>
        {#if setting.value !== null}
          <Button
            variant="ghost"
            type="button"
            size="sm"
            disabled={busyKey !== null}
            onclick={() => void clear(setting)}
          >
            {t('network-setting-clear')}
          </Button>
        {/if}
      </div>
      {#if hintOf(setting)}
        <p class="hint">{hintOf(setting)}</p>
      {/if}
      {#if stateOf(setting)}
        <p class="state" aria-live="polite">{stateOf(setting)}</p>
      {/if}
      {#if errors[setting.key]}
        <p class="error" role="alert">{errors[setting.key]}</p>
      {/if}
    </div>
  {/each}
</div>

<style>
  .network-settings {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
  }
  .setting {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  .label {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .controls {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }
  .field {
    flex: 1;
    min-width: 0;
  }
  .hint,
  .state {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
    line-height: var(--alloy-typography-line-height-normal, 1.45);
  }
  .state {
    color: var(--alloy-color-text-primary);
    overflow-wrap: anywhere;
  }
  .error {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }
</style>
