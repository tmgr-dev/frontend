#!/usr/bin/env node
// Validates a plugin folder with the same rules the app enforces, before you ever load it. Usage:
//   node validate.mjs [folder] [--json] [--strict]   (folder defaults to the current directory)
// --json prints one machine-readable report; --strict exits 1 on warnings too.
import { existsSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
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

  for (const view of manifest.contributes.views) {
    if (view.ui && !existsSync(join(folder, view.ui))) {
      errors.push(`views.${view.id}: ${view.ui} does not exist`);
    }
  }
  const mainPath = join(folder, manifest.main);
  if (!existsSync(mainPath)) {
    errors.push(`main: ${manifest.main} does not exist`);
  }
  if (errors.length) return { ok: false, errors, warnings, manifest };

  const code = readFileSync(mainPath, 'utf8');

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

  const pagesCode = manifest.contributes.views
    .filter((view) => view.ui)
    .map((view) => readFileSync(join(folder, view.ui), 'utf8'))
    .join('\n');
  const all = `${code}\n${pagesCode}`;
  const callArgs = [...all.matchAll(/\bcall\(\s*['"`]([\w.]+)['"`]/g)].map(
    (match) => `tmgr.${match[1]}`,
  );

  for (const [call, permission] of Object.entries(CALL_PERMISSIONS)) {
    if (all.includes(call) || callArgs.includes(call)) {
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
  if (/type\s*:\s*['"]link['"]/.test(all)) {
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
    /\btmgr\s*\[|\b(?:const|let|var)\s+\w+\s*=\s*tmgr\s*[;,]|\}\s*=\s*tmgr\s*[;,]/.test(
      all,
    ) || /\bcall\(\s*[^'"`\s)]/.test(all);
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
