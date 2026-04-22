# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
npm run dev        # Vite dev server (PWA enabled in dev via vite-plugin-pwa)
npm run build      # Production build (terser, manual vendor chunks, drops console.*)
npm run preview    # Preview built bundle
npm test           # Jest (ts-jest, node env) — runs ./tests
npx jest path/to/file.test.ts   # Run a single test file
npm run format     # Prettier (singleQuote, trailingComma: all, printWidth 80)
```

Env: copy `.env.example` to `.env`. The required var is `VITE_API_BASE_URL`. Pusher/ntfy/Telegram vars are needed only if exercising those features.

## Architecture

Vue 3 SPA for a Laravel-backed task manager (TMGR). Path alias `@/*` → `src/*`.

### Mixed JS/TS by purpose
The codebase splits languages by role — keep this convention when adding files:
- **TypeScript**: `src/actions/tmgr/*` (API layer), `src/composable/*`, `src/utils/*`, `src/types/*`.
- **JavaScript**: `src/router/*`, `src/store/*` (Vuex), `src/mixins/*`, and most `.vue` `<script>` blocks.
- Vue components use **Options API with `defineComponent`**. The Composition API is used inside `setup()` (never `<script setup>`). This is enforced project-wide.

### Data flow: API → Cache → Vuex → Components
The performance layer in `src/utils/` is load-bearing — actions in `src/actions/tmgr/*` are expected to use it:

1. **`requestCache`** (`src/utils/requestCache.ts`) — TTL-keyed in-memory cache. Reference data (workspaces, statuses, categories, members) is cached for 5 min; per-task data for ~1 min. **Mutating actions must call `requestCache.invalidate(key | regex)`** or stale data will render. `store.dispatch('logout')` clears the cache.
2. **`requestDeduplicator`** (`src/utils/requestDeduplicator.ts`) — collapses concurrent identical GETs into a single in-flight Promise. Wrap network calls in `requestDeduplicator.deduplicate(key, fn)` after the cache miss.
3. **`$axios`** (`src/plugins/axios.ts`) — single instance with 30s timeout, bearer token from `store.state.token`, auto-retry (×2, exponential) on 5xx, and auto-`logout` dispatch on 401.

### Vuex (root store + modules)
`src/store/index.js` holds auth/session/UI flags; modules live in `src/store/modules/` (`pusher`, `boardFilters`, `featureToggles`). Reference collections are kept **normalized**: `workspaces` + `workspacesById`, `workspaceStatuses` + `workspaceStatusesById`. Prefer the `*ById` lookups and the `currentWorkspace` / `userSettingByKey` getters over scanning arrays.

Modal state is controlled via root mutations: `setCurrentTaskIdForModal`, `setShowCreatingTaskModal`, `closeTaskModal`. The modal stack (`modalStack`, `openModals`) is central — don't manage modal visibility with local refs alone if it competes for ESC/backdrop handling.

`colorScheme` (light/dark) is mirrored to `localStorage` and toggles the `dark` class on `<html>` via the `setColorScheme` mutation.

### Routing
`src/router/index.js` resolves the user's landing page on `/` for logged-in users: it fetches user → workspaces → user feature toggles → workspace feature toggles, then redirects to `/{workspace.code}/{landing}` where `landing` ∈ `list | board | dashboard` (gated by workspace toggles). All app URLs are workspace-scoped: `/:workspace_code/...`.

Route meta flags drive the guard:
- `allowedGuests: true` — public route (login, register, etc.).
- `notOnlyForLoggedUsers: true` — also accessible when authenticated.
- Anything else requires auth and redirects to `Login` when token is missing.

### Real-time & notifications
- `src/store/modules/pusher.js` + `src/store/plugins/pusher.js` — Pusher/Soketi channel subscriptions, paired with the `usePusher` composable.
- `src/store/plugins/ntfy-client.js` and `pusher-beams-client.js` — push notification transports.
- `usePusher`, `useNotifications`, `useNetworkStatus` composables wrap subscription lifecycles.

### Build chunking
`vite.config.ts` defines manual vendor chunks: `vendor-vue`, `vendor-editor` (Editor.js + plugins), `vendor-ui` (Radix/Headless/Lucide), `vendor-utils`, `vendor-markdown`, `vendor-pusher`. When adding a heavy dependency, slot it into the appropriate chunk to avoid bloating the entry. Production strips `console.log/info/debug` via terser.

PWA is configured in `vite.config.ts` with `NetworkFirst` for `/api/*` (5min TTL, 10s network timeout) and `CacheFirst` for Google Fonts. Service worker imports Pusher Beams' SW.

## Conventions

- **Don't use `<script setup>`** — the project standardizes on `defineComponent({ setup() { ... } })` for Composition API and Options API otherwise.
- **Don't rename existing methods/fields or do drive-by refactors** — the `.cursorrules` calls this out explicitly; respect existing names even if they look improvable.
- **Don't write comments** unless the *why* is genuinely non-obvious.
- **Dark mode**: every visual change needs the `dark:` variant.
- **shadcn-vue components** live in `src/components/ui/`; brand colors `tmgr-blue`, `tmgr-light-blue`, `tmgr-gray` are defined in `tailwind.config.js`.
- **Workspace context** must be in URLs and API calls — pull `currentWorkspace.code` from the store rather than hardcoding.
- The full style guide (`.cursorrules`) has extensive examples for Options API, Vuex modules, composables, and shadcn usage; consult it for boilerplate but treat the points above as the binding constraints.
