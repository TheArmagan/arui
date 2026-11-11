# Paper theme

An ARUI mod. `src/index.ts` exports `activate(ctx)`; ARUI calls it once in every shell window
(taskbar, start menu, popups, the hidden host window). Use `ctx.surface.id` to tell them apart.

## Commands

```sh
pnpm install
pnpm build    # one-off build into ./dist
pnpm watch    # rebuild ./dist on change
pnpm dev      # rebuild into %APPDATA%\ARUI\mods\theme-paper (or $ARUI_MODS_DIR/theme-paper); ARUI hot reloads it
pnpm check    # type-check with svelte-check
```

## Rules

- `svelte`, `svelte/*` and `@arui/sdk(/*)` must stay external (see `vite.config.ts`). ARUI maps
  them to its own copies, so keep `svelte` on the same version as ARUI (5.57.3).
- `mod.json` `apiVersion` must match the SDK's `API_VERSION`.
- Styles: Svelte component `<style>` blocks end up in `dist/style.css`, which `mod.json` lists.
  Use the `--arui-*` CSS variables (`--arui-bg`, `--arui-fg`, `--arui-border`, `--arui-radius`, ...)
  so the mod follows the active theme.
- Shared components (Button, Card, ContextMenu, Tooltip, Slot, ContextMenuHost) come from
  `@arui/sdk/ui`.
