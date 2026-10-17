# Common Components

Foundation components for Alloy UI, implementing decision 0001 (text-first, colored user identity, Telegram-inspired).

## Components

### Avatar

Circular avatar with initials and optional image. Supports presence indicators.

**Props:**
- `userId?: string` - Matrix user ID for color derivation
- `displayName?: string | null` - Display name for initials
- `src?: string | null` - Image URL (fallback to initials)
- `size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl'` - Default: `'md'` (36px)
- `presence?: 'online' | 'away' | 'busy' | 'offline' | null` - Colored dot indicator
- `alt?: string` - Image alt text (derived from displayName/userId if not provided)
- `class?: string` - CSS class

### UserName

Colored sender name. Uses deterministic color from Matrix user ID.

**Props:**
- `userId: string` - Matrix user ID (required)
- `displayName?: string | null` - Display name (defaults to userId)
- `class?: string` - CSS class

### Toggle

Accessible switch/toggle control with animation and reduced-motion support.

**Props:**
- `checked?: boolean` - Bindable state, default `false`
- `label: string` - Aria-label (required)
- `disabled?: boolean` - Default `false`
- `onchange?: (checked: boolean) => void` - Change callback
- `class?: string` - CSS class

**Keyboard:** Space, Enter to toggle. Focus-visible outline.

### SegmentedControl

Radio group with roving tabindex and keyboard navigation (arrows, Home, End).

**Props:**
- `options: Array<{ value: string; label: string }>` - Option list (required)
- `value?: string` - Bindable selected value
- `label?: string` - Aria-label for group
- `disabled?: boolean` - Default `false`
- `onchange?: (value: string) => void` - Change callback
- `class?: string` - CSS class

**Keyboard:** Arrow Left/Right, Up/Down to navigate. Home/End to jump.

### Badge

Unread count badge with optional muted variant. Does not render if count is 0 or undefined.

**Props:**
- `count?: number | string` - Badge count/label
- `variant?: 'default' | 'muted'` - Default: `'default'` (accent color)
- `max?: number` - Threshold for `{max}+` display (default: 99)
- `class?: string` - CSS class

### Button

Text-first button with optional leading icon. Variants: primary (accent), ghost (subtle), danger (red).

**Props:**
- `variant?: 'primary' | 'ghost' | 'danger'` - Default: `'primary'`
- `size?: 'sm' | 'md' | 'lg'` - Default: `'md'`
- `disabled?: boolean` - Default `false`
- `type?: 'button' | 'submit' | 'reset'` - Default: `'button'`
- `icon?: Component` - Lucide icon component (optional)
- `ariaLabel?: string` - Aria-label (optional)
- `onclick?: (event: MouseEvent) => void` - Click handler
- `children?: Snippet` - Button text/content
- `class?: string` - CSS class

## Tokens Used

- `--alloy-color-accent-primary`: Primary action, toggle checked state
- `--alloy-color-surface-canvas`, `--alloy-color-surface-subtle`, `--alloy-color-surface-hover`, `--alloy-color-surface-panel`: Background layers
- `--alloy-color-text-primary`, `--alloy-color-text-secondary`, `--alloy-color-text-on-accent`: Text colors
- `--alloy-color-user-1..12`: User identity colors
- `--alloy-color-status-{online,away,busy,offline,error}`: Status indicators
- `--alloy-radii-*`: Border radius
- `--alloy-typography-*`: Font sizes, weights
- `--alloy-motion-duration-fast`: Transition timing
- `--alloy-elevation-sm`: Shadow for depth

## Testing

Each component includes keyboard event handlers and ARIA roles. Test with `@testing-library/svelte` or Vitest component tests.

## Design Rules (Decision 0001)

- No decorative emoji icons anywhere in the component library
- Icons (Lucide) are text-only when UI affords text labels
- User colors are applied to sender names (UserName) and avatar backgrounds (Avatar)
- No color-only identity: names are always shown
- Toggle for theme switching (in Shell); segmented controls for density/layout options
- Tailored for light, dark, and AMOLED themes
