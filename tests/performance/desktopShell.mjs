import { mockApp } from './mockApp.mjs';

/** The desktop shell's local-workspace commands, run on node:sqlite (Node ≥ 22) instead of Rust. */
const fakeShell = async () => {
  const { DatabaseSync } = await import('node:sqlite');
  const workspaces = [];
  const dbs = new Map();
  const exports = [];
  const revealed = [];
  const files = new Map();
  const db = (code) => {
    if (!dbs.has(code)) dbs.set(code, new DatabaseSync(':memory:'));
    return dbs.get(code);
  };
  const handler = async (command, args = {}) => {
    switch (command) {
      case 'local_workspaces_list':
        return workspaces;
      case 'local_workspace_create': {
        const code = args.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
        const workspace = {
          id: -1000 - workspaces.length,
          name: args.name,
          code,
          schema_version: 0,
          created_at: new Date().toISOString(),
          path: `/tmp/${code}`,
          database: `/tmp/${code}/workspace.db`,
        };
        workspaces.push(workspace);
        return workspace;
      }
      case 'local_db_select':
        return db(args.code)
          .prepare(args.sql)
          .all(...args.params);
      case 'local_db_execute': {
        const result = db(args.code)
          .prepare(args.sql)
          .run(...args.params);
        return {
          rowsAffected: Number(result.changes),
          lastInsertId: Number(result.lastInsertRowid),
        };
      }
      case 'local_file_write':
        files.set(args.headers['x-tmgr-target'], Buffer.from(args.raw));
        return null;
      case 'reveal_download':
        revealed.push(args.path);
        return null;
      case 'local_export_write':
        exports.push(args);
        return `/tmp/${args.code}/exports/${args.folder}`;
      default:
        return null;
    }
  };
  return Object.assign(handler, { exports, files, revealed });
};

/** A page that believes it runs inside the desktop app, backed by `fakeShell` and a fake file scheme. */
export const desktopPage = async (page, mockOptions = {}) => {
  const shell = await fakeShell();
  await page.exposeFunction('__shellInvoke', (command, args) =>
    shell(command, args),
  );
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      invoke: (command, args, options) => {
        if (command === 'plugin:event|listen') {
          (window.__listeners[args.event] ??= []).push(args.handler);
          return Promise.resolve(args.handler);
        }
        if (command.startsWith('plugin:event|')) return Promise.resolve(null);
        return window.__shellInvoke(
          command,
          args instanceof Uint8Array
            ? { raw: Array.from(args), headers: options?.headers ?? {} }
            : args ?? {},
        );
      },
      convertFileSrc: (path, protocol) =>
        `${location.origin}/__${protocol}/${encodeURIComponent(path)}`,
      transformCallback: (callback) => {
        const id = (window.__callbackId = (window.__callbackId ?? 0) + 1);
        window[`__callback${id}`] = callback;
        return id;
      },
    };
    window.__listeners = {};
    window.__emit = (event, payload) =>
      (window.__listeners[event] ?? []).forEach((id) =>
        window[`__callback${id}`]?.({ event, id, payload }),
      );
  });
  await mockApp(page, mockOptions);
  const stored = shell.files;
  await page.route('**/__tmgrfile/**', async (route) => {
    const key = decodeURIComponent(
      new URL(route.request().url()).pathname.replace('/__tmgrfile/', ''),
    );
    const method = route.request().method();
    if (method === 'PUT') {
      // WKWebView hands a custom scheme handler no fetch() body.
      await route.fulfill({ status: 400, body: 'empty file body' });
    } else if (method === 'DELETE') {
      stored.delete(key);
      await route.fulfill({ status: 204, body: '' });
    } else if (stored.has(key)) {
      await route.fulfill({
        status: 200,
        contentType: 'image/png',
        body: stored.get(key),
      });
    } else {
      await route.fulfill({ status: 404, body: '' });
    }
  });
  return shell;
};
