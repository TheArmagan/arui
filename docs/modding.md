# Modding ARUI

Everything you see in ARUI is a mod. The taskbar, start menu, tray, clock, media card, previews and
settings page are built-in mods in `renderers/ui/src/builtin/`, and they use the same API that your
mod gets. If a built-in mod can do something, yours can too, including replacing it.

## What a mod can do

- **Widgets**: add Svelte components to named slots (`taskbar.left`, `taskbar.center`,
  `taskbar.right`, `taskbar.end`, `start.header`, `start.footer`, `tray.flyout`, `quick.settings`, ...)
  or hide another mod's widget by id.
- **Surfaces**: define new windows (`appbar` docked to a screen edge and reserving space, `popup`
  that closes on blur, or a free `panel`), or replace an existing surface such as `taskbar` or
  `start-menu` with your own component.
- **Themes**: register CSS or `--arui-*` token overrides and switch between them.
- **Commands, events, context menu items, settings**: settings declared with a schema get a form in
  the ARUI settings page automatically.
- **Native data and actions**: reactive `ctx.state` (windows, app groups, pins, start menu apps,
  tray icons, media sessions, volume, network, monitors, clock) and `ctx.actions` (focus, minimize,
  close, launch, pin, media control, volume, live window previews). `ctx.invoke` gives raw access to
  every host command in `renderers/sdk/src/protocol.ts`.

Mods are fully trusted: they run in the shell's webviews with the full API. Only install mods you
trust.

The complete typed API is `renderers/sdk/src/api.ts`. Read it; it is short and commented.

## Create a mod

```sh
node renderers/create-arui-mod my-mod --name "My Mod"
cd my-mod
pnpm install
pnpm dev     # builds in watch mode into %APPDATA%\ARUI\mods\my-mod
```

With ARUI running, the mod loads immediately and reloads every time you save. Open ARUI settings to
enable or disable it.

A mod folder looks like this (the folder name must equal `id`):

```
%APPDATA%\ARUI\mods\my-mod\
  mod.json          { "id": "my-mod", "name": "My Mod", "version": "0.1.0", "apiVersion": 1,
                      "entry": "dist/index.js", "styles": ["dist/style.css"] }
  dist/index.js     ES module exporting activate(ctx) (default export via defineMod)
  dist/style.css
```

`svelte` and `@arui/sdk` must stay external in your bundle (the template does this). ARUI maps them
to its own copies with an import map so that your components share the shell's Svelte runtime.
Keep your `svelte` version equal to the one in `renderers/ui/package.json`.

## Examples

`examples/mods/` has three mods built with the template:

- `theme-paper`: a light theme, tokens and a little CSS.
- `focus-timer`: a taskbar widget with a settings schema and taskbar context menu items.
- `side-dock`: replaces the whole `taskbar` surface with a vertical dock on the left or right edge.

## Execution model

`activate(ctx)` runs once in every ARUI window: each per-monitor taskbar, each popup, and a hidden
`host` window. `ctx.surface.id` tells you which window you are in. Registrations are per window and
are undone when the mod is disabled or hot reloaded (return a dispose function or use
`ctx.onDispose` for your own cleanup). Surface definitions are collected in the `host` window and
sent to the native host, which creates the actual windows.

## Trying things without Windows natives

`pnpm dev:ui` and open `http://localhost:5169/?surface=dev`: a fake desktop with a mock backend
(fake windows, apps, tray, media) renders every surface. The example mods are served there too,
which is the fastest way to iterate on UI.
