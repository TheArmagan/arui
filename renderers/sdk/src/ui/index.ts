// @arui/sdk/ui: shared Svelte components for mods. Inside ARUI this module resolves (via the
// import map) to the copy the core UI already loaded, so styling and state are shared.
// Components are styled by the core Tailwind build and the --arui-* theme tokens.

export { cn } from './utils';
export type { WithElementRef, WithoutChild, WithoutChildren, WithoutChildrenOrChild } from './utils';

export {
	Button,
	buttonVariants,
	type ButtonProps,
	type ButtonSize,
	type ButtonVariant
} from './button/index';
export * as Card from './card/index';
export * as ContextMenu from './context-menu/index';
export * as Tooltip from './tooltip/index';

export { default as Slot } from './Slot.svelte';
export { default as ContextMenuHost } from './ContextMenuHost.svelte';
export { default as MenuItems } from './MenuItems.svelte';
export { default as MenuIcon } from './MenuIcon.svelte';
export { default as WidgetMount } from './WidgetMount.svelte';
export { ARUI_CTX_KEY, ARUI_HOST_KEY, elementScreenRect, getArui, getUiHost } from './host';
export type { MenuPoint, SlotItem, UiHost } from './host';
