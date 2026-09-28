import { mockApp } from './mockApp.mjs';

/** The desktop shell's local-workspace commands, run on node:sqlite (Node ≥ 22) instead of Rust. */
const fakeShell = async ({
  devPlugins = [],
  localHttp = () => ({ status: 404, headers: [], body: '' }),
  releases = {},
} = {}) => {
  const { createHash } = await import('node:crypto');
  const installed = new Map();
  const catalog = { serial: 0, plugins: [], blocked: [] };
  const blockedReason = (plugin) =>
    catalog.blocked.find(
      (block) => block.id === plugin.id || block.repo === plugin.repo,
    )?.reason ?? null;
  const { DatabaseSync } = await import('node:sqlite');
  const workspaces = [];
  const dbs = new Map();
  const exports = [];
  const revealed = [];
  const fetches = [];
  const picks = [];
  const pages = new Map();
  const windows = [];
  const replies = [];
  const closed = [];
  const files = new Map();
  const tokens = new Map();
  const accessReplies = [];
  let tokenSeq = 0;
  let accessEnabled = false;
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
      case 'plugin_github_release': {
        const bundle = releases[args.repo];
        if (!bundle)
          throw new Error(
            `the latest release of ${args.repo} has no tmgr-plugin.json`,
          );
        const text = JSON.stringify(bundle.content);
        return {
          repo: args.repo,
          tag: bundle.tag,
          sha256: createHash('sha256').update(text).digest('hex'),
          bundle: text,
          signature: 'untrusted comment: test\n',
          public_key: 'RWtestkey',
          verified: Boolean(bundle.verified),
        };
      }
      case 'plugin_install': {
        const reason = blockedReason(args.plugin);
        if (reason) throw new Error(`this plugin is blocked: ${reason}`);
        installed.set(args.plugin.id, args.plugin);
        return null;
      }
      case 'plugins_installed_list':
        return [...installed.values()].map((plugin) => ({
          ...plugin,
          blocked: blockedReason(plugin),
        }));
      case 'plugin_catalog':
      case 'plugin_catalog_refresh':
        return catalog;
      case 'plugin_uninstall':
        installed.delete(args.id);
        return null;
      case 'plugin_page_put':
        pages.set(args.key, args.html);
        return null;
      case 'plugin_window_open':
        windows.push(args);
        return null;
      case 'plugin_window_reply':
        replies.push(args);
        return null;
      case 'plugin_windows_close':
        closed.push(args.pluginId);
        return null;
      case 'plugin_pick_file':
        picks.push(args.title);
        return { name: 'picked.txt', size: 2, base64: 'aGk=' };
      case 'plugin_fetch':
        fetches.push(args.request);
        return localHttp(args.request);
      case 'plugins_dev_list':
        return devPlugins;
      case 'reveal_download':
        revealed.push(args.path);
        return null;
      case 'local_export_write':
        exports.push(args);
        return `/tmp/${args.code}/exports/${args.folder}`;
      case 'local_token_issue': {
        const id = `local-token-${++tokenSeq}`;
        const now = Date.now();
        const token = {
          id,
          prefix: `tmgrl_${id}`,
          personaUuid: args.personaUuid,
          personaName: args.personaName,
          workspaceCode: args.workspaceCode,
          workspaceId: -1000,
          label: args.label,
          createdAt: new Date(now).toISOString(),
          expiresAt: new Date(
            now + args.expiresInDays * 86400000,
          ).toISOString(),
          lastUsedAt: null,
          revokedAt: null,
          pluginId: args.pluginId ?? null,
        };
        tokens.set(id, token);
        accessEnabled = true;
        return token;
      }
      case 'local_token_list':
        return [...tokens.values()].filter(
          (t) => !args.workspaceCode || t.workspaceCode === args.workspaceCode,
        );
      case 'local_token_revoke': {
        const token = tokens.get(args.id);
        if (!token) return false;
        token.revokedAt = new Date().toISOString();
        return true;
      }
      case 'local_token_revoke_all': {
        let count = 0;
        for (const token of tokens.values()) {
          if (token.revokedAt) continue;
          if (args.personaUuid && token.personaUuid !== args.personaUuid) continue;
          if (args.pluginId && token.pluginId !== args.pluginId) continue;
          if (args.workspaceCode && token.workspaceCode !== args.workspaceCode) continue;
          token.revokedAt = new Date().toISOString();
          count++;
        }
        return count;
      }
      case 'local_access_status':
        return {
          enabled: accessEnabled,
          listening: false,
          socketPath: null,
          safeMode: false,
          ready: true,
          bridgeCommand: '/tmp/tmgr-desktop',
        };
      case 'local_access_reply':
        accessReplies.push(args);
        return null;
      case 'local_access_set_enabled':
        accessEnabled = !!args.enabled;
        return null;
      default:
        return null;
    }
  };
  return Object.assign(handler, {
    exports,
    files,
    revealed,
    fetches,
    picks,
    pages,
    windows,
    replies,
    closed,
    installed,
    catalog,
    tokens,
    accessReplies,
  });
};

/** A page that believes it runs inside the desktop app, backed by `fakeShell` and a fake file scheme. */
export const desktopPage = async (
  page,
  mockOptions = {},
  shellOptions = {},
) => {
  const shell = await fakeShell(shellOptions);
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
