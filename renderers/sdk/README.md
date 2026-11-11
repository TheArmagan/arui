# @arui/sdk

Types and shared components for ARUI mods.

- `@arui/sdk`: the protocol types (`types.ts`, `protocol.ts`), the mod API (`api.ts`), `defineMod`,
  `API_VERSION` and `iconUrl`.
- `@arui/sdk/ui`: Svelte components that mods can render inside ARUI surfaces: `Button`, `Card`,
  `ContextMenu`, `Tooltip` (ported from shadcn-svelte, styled by the shell's Tailwind build and the
  `--arui-*` theme tokens), plus `Slot`, `ContextMenuHost`, `getArui()` and `getUiHost()`.

Inside ARUI both entry points resolve through the shell's import map to the copies the shell
already loaded, so a mod must not bundle them (or `svelte`). The `create-arui-mod` template sets
the externals up.

## Writing a mod

```ts
import { defineMod } from '@arui/sdk';
import Widget from './Widget.svelte';

export default defineMod({
	activate(ctx) {
		ctx.widgets.register('taskbar.right', Widget, { id: 'my-mod:widget', order: 400 });
	}
});
```

`activate` runs once in every shell window (each taskbar, popup and the hidden `host` window);
`ctx.surface.id` says which. Everything registered through `ctx` is removed when the mod is
disabled or hot reloaded.

Every widget and surface component receives its mod's `ctx` as a prop, and can also call
`getArui()` from `@arui/sdk/ui` during initialisation.

## Conventions used by the core mods

These are not part of the typed API yet:

- Local events raised by the runtime, readable with `ctx.events.on(name)`:
  `shell.startKey`, `shell.explorerRestarted`, `shell.fullscreen`, `tray.balloon`,
  `surface.shown`, `surface.hidden`.
- Commands registered by core mods in each window: `core.previews:hover` /
  `core.previews:leave` (`{ appId, rect, monitor }`) to show window previews from a custom
  taskbar, and `arui.runtime` (registered by the runtime) which returns mod settings schemas,
  load errors and the theme list.
- Element rects for `ctx.surfaces.open({ rect })` must be screen pixels: use
  `getUiHost().screenRect(el.getBoundingClientRect())`.
- Core surfaces expose stable class names for themes, for example `.arui-taskbar-bar`.
