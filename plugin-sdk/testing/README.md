# tmgr-plugin-testing

Runs a TMGR plugin's `main.js` in a real QuickJS sandbox, wired to the app's own broker (permission
checks, parameter validation) and manifest validation — backed by an in-memory mock of TMGR's data
instead of the real API.

```js
const { createTestHost } = require('tmgr-plugin-testing');
const manifest = require('./manifest.json');

const host = await createTestHost({
	manifest,
	mainPath: require.resolve('./main.js'),
	statuses: [{ name: 'Active', type: 'active' }],
});

await host.runCommand('yourname.hello.refresh', null);
console.log(host.tmgr.statusBar);
host.dispose();
```

See `index.d.ts` for the full `createTestHost` options and the returned host's shape (`tmgr` state,
`emit`, `runCommand`, `renderPage`, `renderSection`, `badges`, `fireAlarms`, `calls`, `dispose`).
`tmgr.routines`/`tmgr.routineInstances` back `tmgr.routines.*` (local workspaces only); seed them with
the `routines`/`routineInstances` options.

`renderPage`/`renderSection` sanitize with the manifest's own `apiMinor` (parsed from `engines.tmgr`):
a plugin declaring `^1.3` gets the API 1.3 `card`/`grid`/`menu` nodes and the new optional fields on
`stack`/`button`/`stat`/`badge`; an older `engines.tmgr` gets those nodes downgraded to plain `stack`s,
same as the real app.

`tmgr.viewBadges[viewId]` holds what `tmgr.ui.setViewBadge` set (`{ count, text, tone }`, exactly one of
`count`/`text` non-null). The test host applies it immediately: the app coalesces calls to one per second
per view, the test host does not. Validation and errors are the app's own. One difference: the app answers
a refused `setTrayItem` (bad params, tray not allowed here) with a log line instead of a rejection so a page
render is never broken by it; the test host rejects, so you see the mistake.

## Why CommonJS

This package is plain `require()`-able CommonJS, not ESM. A `node:test` file (an ESM `.mjs`, as in
`plugin-sdk/template/main.test.mjs`) can still `import { createTestHost } from 'tmgr-plugin-testing'`
directly — Node resolves a CommonJS package's named exports for ESM importers on its own. CommonJS was
chosen because the TMGR app's own Jest suite (which loads this same package for its kitchen-sink example
test) cannot load real ESM modules without `--experimental-vm-modules`; either way, a plugin author needs
no build step.

## Keeping this in sync with the app

`broker.generated.js`, `manifest.generated.js`, `sandbox.generated.js`, `uiTree.generated.js` and
`prelude.generated.js` are not written by hand. They are transpiled straight from the app's own
`src/pluginSystem/{broker,manifest,sandbox,uiTree}.ts` by `sync-prelude.mjs`, run from the app repo:

```bash
node plugin-sdk/testing/sync-prelude.mjs          # regenerate
node plugin-sdk/testing/sync-prelude.mjs --check  # fail if stale (also run by a Jest test)
```

Those four source files have no Vue or Tauri dependency (`broker.ts` and `manifest.ts` have no runtime
imports at all once their type-only imports are stripped), so a straight TypeScript-to-JS transpile is
byte-for-byte the same logic the app runs — permission checks, parameter validation and manifest rules
never drift from what ships.
