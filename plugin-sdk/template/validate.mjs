#!/usr/bin/env node
// Validates a plugin folder with the same rules the app enforces, before you ever load it. Usage:
//   node validate.mjs [folder] [--json] [--strict]   (folder defaults to the current directory)
// --json prints one machine-readable report; --strict exits 1 on warnings too.
import {
  closeSync,
  existsSync,
  constants as fsConstants,
  fstatSync,
  lstatSync,
  openSync,
  readFileSync,
  readSync,
  realpathSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

// require(), not a named import: manifest.generated.js is CommonJS (see plugin-sdk/testing/README.md),
// and Node's static export detection for a `require`-based CJS module is not reliable enough to import it.
const { parseManifest, PERMISSION_GROUPS } = createRequire(import.meta.url)(
  './manifest.generated.js',
);

// tmgr.<call> -> the permission the app requires for it (see src/pluginSystem/broker.ts).
export const CALL_PERMISSIONS = {
  'tmgr.tasks.list': 'tasks:read',
  'tmgr.tasks.get': 'tasks:read',
  'tmgr.tasks.create': 'tasks:write',
  'tmgr.tasks.update': 'tasks:write',
  'tmgr.tasks.relations': 'relations:read',
  'tmgr.tasks.relate': 'relations:write',
  'tmgr.tasks.unrelate': 'relations:write',
  'tmgr.statuses.list': 'statuses:read',
  'tmgr.statuses.create': 'statuses:write',
  'tmgr.statuses.update': 'statuses:write',
  'tmgr.statuses.reorder': 'statuses:write',
  'tmgr.categories.list': 'categories:read',
  'tmgr.categories.create': 'categories:write',
  'tmgr.categories.update': 'categories:write',
  'tmgr.time.start': 'time:write',
  'tmgr.time.stop': 'time:write',
  'tmgr.comments.list': 'comments:read',
  'tmgr.comments.add': 'comments:write',
  'tmgr.comments.react': 'comments:write',
  'tmgr.files.export': 'files:export',
  'tmgr.files.reveal': 'files:export',
  'tmgr.files.list': 'files:attachments',
  'tmgr.files.read': 'files:attachments',
  'tmgr.files.pick': 'files:pick',
  'tmgr.ui.notify': 'notifications',
  'tmgr.alarms.create': 'alarms',
  'tmgr.alarms.clear': 'alarms',
  'tmgr.alarms.list': 'alarms',
  'tmgr.ui.setTrayItem': 'tray',
  'tmgr.ui.setTrayTitle': 'tray',
  'tmgr.ui.setViewBadge': 'views:badge',
  'tmgr.agentWork.list': 'agent_work:read',
  'tmgr.agentWork.start': 'agent_work:write',
  'tmgr.agentWork.update': 'agent_work:write',
  'tmgr.agentWork.finish': 'agent_work:write',
  'tmgr.pages.search': 'pages:read',
  'tmgr.pages.tree': 'pages:read',
  'tmgr.pages.get': 'pages:read',
  'tmgr.pages.create': 'pages:write',
  'tmgr.pages.update': 'pages:write',
  'tmgr.pages.append': 'pages:write',
  'tmgr.pages.setSection': 'pages:sections',
  'tmgr.pageData.get': 'pages:read',
  'tmgr.pageData.getMany': 'pages:read',
  'tmgr.pageData.set': 'pages:write',
  'tmgr.pageData.delete': 'pages:write',
  'tmgr.taskData.get': 'tasks:read',
  'tmgr.taskData.getMany': 'tasks:read',
  'tmgr.taskData.set': 'tasks:write',
  'tmgr.taskData.delete': 'tasks:write',
  'tmgr.routines.list': 'routines:read',
  'tmgr.routines.get': 'routines:read',
  'tmgr.routines.instances': 'routines:read',
  'tmgr.routines.create': 'routines:write',
  'tmgr.routines.update': 'routines:write',
  'tmgr.routines.complete': 'routines:write',
  'tmgr.routines.skip': 'routines:write',
  'tmgr.routines.convertToTask': 'routines:write',
};

// tmgr.events.on(<event>) -> the permission the app requires for it (PLUGIN_EVENTS in src/pluginSystem/broker.ts).
export const EVENT_PERMISSIONS = {
  'task.created': 'tasks:read',
  'task.updated': 'tasks:read',
  'task.deleted': 'tasks:read',
  'task.statusChanged': 'tasks:read',
  'timer.started': 'time:read',
  'timer.stopped': 'time:read',
  'comment.created': 'comments:read',
  'comment.updated': 'comments:read',
  'comment.deleted': 'comments:read',
  'comment.reactionChanged': 'comments:read',
  'task.relationChanged': 'relations:read',
  'routine.created': 'routines:read',
  'routine.updated': 'routines:read',
  'routine.deleted': 'routines:read',
  'page.created': 'pages:read',
  'page.updated': 'pages:read',
  'page.deleted': 'pages:read',
  'page.restored': 'pages:read',
  'page.moved': 'pages:read',
  alarm: 'alarms',
};

export const MAX_FILE_BYTES = 1024 * 1024;

// The app keeps catalog.json, catalog.json.tmp and installed/ in the plugins folder, and skips dot folders.
export const isReservedId = (id) =>
  id === 'catalog.json' ||
  id === 'installed' ||
  id.startsWith('.') ||
  id.endsWith('.tmp');

// Reads at most MAX_FILE_BYTES + 1 bytes from an fd opened without following a final symlink,
// so a file swapped for a symlink or a device after the check still cannot be read past the cap.
export const readSource = (path, label = path) => {
  let stat = null;
  try {
    stat = lstatSync(path);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    throw new Error(`${label} does not exist`);
  }
  if (stat.isSymbolicLink()) throw new Error(`${label} is a symlink, refusing`);
  let fd;
  try {
    fd = openSync(
      path,
      fsConstants.O_RDONLY |
        (fsConstants.O_NOFOLLOW ?? 0) |
        (fsConstants.O_NONBLOCK ?? 0),
    );
  } catch (error) {
    if (error.code === 'ELOOP') {
      throw new Error(`${label} is a symlink, refusing`);
    }
    throw error;
  }
  try {
    if (!fstatSync(fd).isFile()) {
      throw new Error(`${label} is not a regular file`);
    }
    const buffer = Buffer.alloc(MAX_FILE_BYTES + 1);
    let length = 0;
    while (length < buffer.length) {
      const read = readSync(fd, buffer, length, buffer.length - length, null);
      if (!read) break;
      length += read;
    }
    if (length > MAX_FILE_BYTES) {
      throw new Error(
        `${label} is over the limit: the app ignores files over ${MAX_FILE_BYTES} bytes`,
      );
    }
    return buffer.subarray(0, length);
  } finally {
    closeSync(fd);
  }
};

// The path of a file named by the manifest, after checking its real location is inside the folder.
export const resolveInside = (folder, name, label) => {
  const root = realpathSync(folder);
  const lexical = resolve(root, name);
  const outside = () =>
    new Error(`${label} "${name}" points outside the plugin folder`);
  const within = (path) => {
    const rel = relative(root, path);
    return (
      rel !== '' &&
      rel !== '..' &&
      !rel.startsWith(`..${sep}`) &&
      !isAbsolute(rel)
    );
  };
  if (!within(lexical)) throw outside();
  try {
    if (lstatSync(lexical).isSymbolicLink()) {
      throw new Error(`${label} "${name}" is a symlink, refusing`);
    }
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    throw new Error(`${label}: ${name} does not exist`);
  }
  if (!within(realpathSync(lexical))) throw outside();
  return lexical;
};

const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

export const validate = (folder) => {
  const errors = [];
  const warnings = [];

  const manifestPath = join(folder, 'manifest.json');
  if (!existsSync(manifestPath)) {
    errors.push(`no manifest.json in ${folder}`);
    return { ok: false, errors, warnings };
  }

  let manifest;
  try {
    manifest = parseManifest(JSON.parse(readFileSync(manifestPath, 'utf8')));
  } catch (error) {
    errors.push(error instanceof Error ? error.message : String(error));
    return { ok: false, errors, warnings };
  }

  if (isReservedId(manifest.id)) {
    errors.push(`id ${manifest.id} is reserved`);
    return { ok: false, errors, warnings, manifest };
  }

  for (const view of manifest.contributes.views) {
    if (view.ui && !existsSync(join(folder, view.ui))) {
      errors.push(`views.${view.id}: ${view.ui} does not exist`);
    }
  }
  if (!existsSync(join(folder, manifest.main))) {
    errors.push(`main: ${manifest.main} does not exist`);
  }
  if (errors.length) return { ok: false, errors, warnings, manifest };

  let code;
  let pagesCode;
  try {
    code = readSource(
      resolveInside(folder, manifest.main, 'main'),
      manifest.main,
    ).toString('utf8');
    pagesCode = manifest.contributes.views
      .filter((view) => view.ui)
      .map((view) =>
        readSource(
          resolveInside(folder, view.ui, `views.${view.id}`),
          view.ui,
        ).toString('utf8'),
      )
      .join('\n');
  } catch (error) {
    errors.push(error instanceof Error ? error.message : String(error));
    return { ok: false, errors, warnings, manifest };
  }

  for (const command of manifest.contributes.commands) {
    const pattern = new RegExp(
      `commands\\.register\\(\\s*['"\`]${escapeRegExp(command.id)}['"\`]`,
    );
    if (!pattern.test(code)) {
      warnings.push(
        `command ${command.id} is declared but ${manifest.main} does not seem to call tmgr.commands.register('${command.id}', ...)`,
      );
    }
  }

  const granted = new Set(manifest.permissions);
  const used = new Set();
  const need = (permission, message) => {
    used.add(permission);
    if (!granted.has(permission)) warnings.push(message);
  };

  const stripped = code
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/(^|[\s;{}(,])\/\/[^\n]*/g, '$1');
  const all = `${stripped}\n${pagesCode}`;
  const callArgs = [
    ...all.matchAll(/(?:\btmgr\.|(?<![\w.]))call\(\s*['"`]([\w.]+)['"`]/g),
  ].map((match) => `tmgr.${match[1]}`);

  for (const [call, permission] of Object.entries(CALL_PERMISSIONS)) {
    if (
      new RegExp(`${escapeRegExp(call)}(?!\\w)`).test(all) ||
      callArgs.includes(call)
    ) {
      need(
        permission,
        `uses ${call} but does not declare the "${permission}" permission`,
      );
    }
  }
  for (const match of all.matchAll(/events\.on\(\s*['"`]([\w.]+)['"`]/g)) {
    const permission = EVENT_PERMISSIONS[match[1]];
    if (permission) {
      need(
        permission,
        `listens to ${match[1]} but does not declare the "${permission}" permission`,
      );
    }
  }
  if (/\btmgr\.notify\(/.test(all)) {
    need(
      'notifications',
      'uses tmgr.notify but does not declare the "notifications" permission',
    );
  }
  if (
    /['"]?type['"]?\s*:\s*['"]link['"]/.test(all) ||
    manifest.links.allowedDomains.length
  ) {
    need(
      'links:open',
      'seems to render a "link" node but does not declare the "links:open" permission',
    );
  }
  if (manifest.contributes.commands.some((c) => c.deepLink)) {
    need(
      'deeplinks',
      'declares a deep-linkable command but does not declare the "deeplinks" permission',
    );
  }
  if (manifest.contributes.trayItems.length) {
    need(
      'tray',
      'declares contributes.trayItems but does not declare the "tray" permission',
    );
  }
  if (all.includes('tmgr.routines.convertToTask')) {
    need(
      'tasks:write',
      'uses tmgr.routines.convertToTask but does not declare the "tasks:write" permission',
    );
  }
  if (manifest.contributes.menus['task/card'].length) used.add('menus:task');
  if (used.has('pages:write') || used.has('pages:sections')) {
    used.add('pages:read');
  }

  const dynamic =
    /=\s*(?:window\.)?tmgr\b(?!\s*\.\s*\w+\s*\()/.test(all) ||
    /\btmgr\s*(?:\[|\?\.)/.test(all) ||
    /[(,]\s*(?:window\.)?tmgr\s*[,)]/.test(all) ||
    /(?:\btmgr\.|(?<![\w.]))call\(\s*(?!['"`][\w.]+['"`])/.test(all) ||
    /call\(\s*`[^`]*\$\{/.test(all);
  if (dynamic) {
    warnings.push(
      'calls tmgr through a variable or a computed name, so permissions cannot be checked statically; declared-but-unused permissions are not reported',
    );
  } else {
    for (const permission of manifest.permissions) {
      if (!used.has(permission)) {
        warnings.push(
          `declares the "${permission}" permission but nothing in the plugin seems to use it; remove it to ask for less`,
        );
      }
    }
  }

  const groups = Object.entries(PERMISSION_GROUPS)
    .map(([group, members]) => ({
      group,
      permissions: members.filter((p) => granted.has(p)),
    }))
    .filter((entry) => entry.permissions.length);

  return { ok: true, errors, warnings, manifest, groups };
};

const isMain =
  process.argv[1] &&
  resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url));
if (isMain) {
  const args = process.argv.slice(2);
  const json = args.includes('--json');
  const strict = args.includes('--strict');
  const folder = resolve(
    args.find((arg) => !arg.startsWith('--')) ?? process.cwd(),
  );
  const result = validate(folder);
  const failed = !result.ok || (strict && result.warnings.length > 0);
  if (json) {
    console.log(
      JSON.stringify({
        folder,
        ok: !failed,
        errors: result.errors,
        warnings: result.warnings,
        permissions: result.groups ?? [],
      }),
    );
  } else {
    result.warnings.forEach((message) => console.warn(`warning: ${message}`));
    result.errors.forEach((message) => console.error(message));
    if (result.ok) {
      for (const { group, permissions } of result.groups) {
        console.log(`${group}: ${permissions.join(', ')}`);
      }
      if (!failed) console.log(`${folder}: manifest and files look good.`);
    }
  }
  if (failed) process.exit(1);
}
