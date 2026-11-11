// Svelte's internal entry points ship without type declarations. The UI only imports them to
// hand their namespaces to mods (see lib/runtime/shared-modules.ts).
declare module 'svelte/internal/client';
declare module 'svelte/internal/disclose-version';
declare module 'svelte/internal/flags/async';
declare module 'svelte/internal/flags/legacy';
declare module 'svelte/internal/flags/tracing';
