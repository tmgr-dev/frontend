#!/usr/bin/env node
// Validates a plugin folder with the same rules the app enforces, before you ever load it. Usage:
//   node validate.mjs [folder]   (defaults to the current directory)
import { existsSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

// require(), not a named import: manifest.generated.js is CommonJS (see plugin-sdk/testing/README.md),
// and Node's static export detection for a `require`-based CJS module is not reliable enough to import it.
const { parseManifest } = createRequire(import.meta.url)('./manifest.generated.js');

// tmgr.<call> -> the permission the app requires for it (see src/pluginSystem/broker.ts).
const CALL_PERMISSIONS = {
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
	'tmgr.agentWork.list': 'agent_work:read',
	'tmgr.agentWork.start': 'agent_work:write',
	'tmgr.agentWork.update': 'agent_work:write',
	'tmgr.agentWork.finish': 'agent_work:write',
	'tmgr.routines.list': 'routines:read',
	'tmgr.routines.get': 'routines:read',
	'tmgr.routines.instances': 'routines:read',
	'tmgr.routines.create': 'routines:write',
	'tmgr.routines.update': 'routines:write',
	'tmgr.routines.complete': 'routines:write',
	'tmgr.routines.skip': 'routines:write',
	'tmgr.routines.convertToTask': 'routines:write',
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
		const pattern = new RegExp(`commands\\.register\\(\\s*['"\`]${escapeRegExp(command.id)}['"\`]`);
		if (!pattern.test(code)) {
			warnings.push(
				`command ${command.id} is declared but ${manifest.main} does not seem to call tmgr.commands.register('${command.id}', ...)`,
			);
		}
	}

	const granted = new Set(manifest.permissions);
	for (const [call, permission] of Object.entries(CALL_PERMISSIONS)) {
		if (code.includes(call) && !granted.has(permission)) {
			warnings.push(`uses ${call} but does not declare the "${permission}" permission`);
		}
	}
	if (/type\s*:\s*['"]link['"]/.test(code) && !granted.has('links:open')) {
		warnings.push(`seems to render a "link" node but does not declare the "links:open" permission`);
	}
	if (manifest.contributes.commands.some((c) => c.deepLink) && !granted.has('deeplinks')) {
		warnings.push('declares a deep-linkable command but does not declare the "deeplinks" permission');
	}
	if (manifest.contributes.trayItems.length && !granted.has('tray')) {
		warnings.push('declares contributes.trayItems but does not declare the "tray" permission');
	}
	if (code.includes('tmgr.routines.convertToTask') && !granted.has('tasks:write')) {
		warnings.push('uses tmgr.routines.convertToTask but does not declare the "tasks:write" permission');
	}

	return { ok: true, errors, warnings, manifest };
};

const isMain = process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url));
if (isMain) {
	const folder = resolve(process.argv[2] ?? process.cwd());
	const { ok, errors, warnings } = validate(folder);
	warnings.forEach((message) => console.warn(`warning: ${message}`));
	if (!ok) {
		errors.forEach((message) => console.error(message));
		process.exit(1);
	}
	console.log(`${folder}: manifest and files look good.`);
}
