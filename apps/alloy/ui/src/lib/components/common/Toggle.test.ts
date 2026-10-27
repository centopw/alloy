import { describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount, type ComponentProps } from 'svelte';
import Toggle from './Toggle.svelte';

/**
 * The switch's behaviour now comes from Bits UI via the vendored shadcn-svelte primitive, so these
 * tests pin the contract call sites rely on: the switch role, the checked state, the change
 * callback and the disabled state. See decision 0012.
 */
function render(props: ComponentProps<typeof Toggle>) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const component = mount(Toggle, { target, props });
  const element = target.querySelector('[role="switch"]');
  if (!element) throw new Error('Toggle did not render a role="switch" element');
  return { target, component, element: element as HTMLElement };
}

describe('Toggle', () => {
  it('renders a switch with the accessible name from the label prop', () => {
    const { target, component, element } = render({ label: 'Follow system' });

    expect(element.tagName).toBe('BUTTON');
    expect(element.getAttribute('aria-label')).toBe('Follow system');
    expect(element.getAttribute('aria-checked')).toBe('false');

    unmount(component);
    target.remove();
  });

  it('flips the checked state and reports it through onchange', () => {
    const onchange = vi.fn();
    const { target, component, element } = render({ label: 'Read receipts', onchange });

    element.click();
    flushSync();
    expect(onchange).toHaveBeenCalledWith(true);
    expect(element.getAttribute('aria-checked')).toBe('true');

    element.click();
    flushSync();
    expect(onchange).toHaveBeenLastCalledWith(false);
    expect(element.getAttribute('aria-checked')).toBe('false');

    unmount(component);
    target.remove();
  });

  it('honours a bound checked value', () => {
    const { target, component, element } = render({ label: 'Typing notices', checked: true });

    expect(element.getAttribute('aria-checked')).toBe('true');

    unmount(component);
    target.remove();
  });

  it('does not toggle when disabled', () => {
    const onchange = vi.fn();
    const { target, component, element } = render({
      label: 'Auto-download media',
      disabled: true,
      onchange
    });

    element.click();
    flushSync();
    expect(onchange).not.toHaveBeenCalled();
    expect(element.getAttribute('aria-checked')).toBe('false');

    unmount(component);
    target.remove();
  });
});
