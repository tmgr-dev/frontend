import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PLUGIN_EVENTS } from '../broker';
import { PERMISSION_GROUPS, PERMISSIONS } from '../manifest';

const sdk = join(__dirname, '../../../plugin-sdk/template');
const validatePath = join(sdk, 'validate.mjs');

const lint = (
	manifest: object,
	main: string,
	pages: Record<string, string> = {},
) => {
	const dir = mkdtempSync(join(tmpdir(), 'tmgr-lint-'));
	writeFileSync(join(dir, 'manifest.json'), JSON.stringify(manifest));
	writeFileSync(join(dir, 'main.js'), main);
	mkdirSync(join(dir, 'ui'));
	for (const [name, html] of Object.entries(pages)) {
		writeFileSync(join(dir, 'ui', name), html);
	}
	let out = '';
	try {
		out = execFileSync('node', [validatePath, dir, '--json'], {
			encoding: 'utf8',
		});
	} catch (error) {
		out = (error as { stdout: string }).stdout;
	}
	return JSON.parse(out) as {
		ok: boolean;
		errors: string[];
		warnings: string[];
		permissions: { group: string; permissions: string[] }[];
	};
};

const base = {
	id: 'acme.lint',
	name: 'Lint',
	version: '1.0.0',
	engines: { tmgr: '^1.10' },
	main: 'main.js',
};

describe('validate.mjs tables', () => {
	it('lists every broker method that needs a permission', () => {
		const source = readFileSync(join(__dirname, '../broker.ts'), 'utf8');
		const fromBroker = Object.fromEntries(
			[...source.matchAll(/'([a-zA-Z.]+)': \{\s*permission: '([^']+)'/g)].map(
				(m) => [`tmgr.${m[1]}`, m[2]],
			),
		);
		const body = readFileSync(validatePath, 'utf8');
		const table = body.slice(
			body.indexOf('CALL_PERMISSIONS = {'),
			body.indexOf('// tmgr.events.on'),
		);
		const fromValidate = Object.fromEntries(
			[...table.matchAll(/'(tmgr\.[a-zA-Z.]+)': '([^']+)'/g)].map((m) => [
				m[1],
				m[2],
			]),
		);
		expect(fromValidate).toEqual(fromBroker);
	});

	it('lists every event that needs a permission', () => {
		const body = readFileSync(validatePath, 'utf8');
		const table = body.slice(body.indexOf('EVENT_PERMISSIONS = {'));
		const fromValidate = Object.fromEntries(
			[...table.matchAll(/'?([\w.]+)'?: '([\w:]+)',/g)]
				.filter((m) => m[1] in PLUGIN_EVENTS)
				.map((m) => [m[1], m[2]]),
		);
		const expected = Object.fromEntries(
			Object.entries(PLUGIN_EVENTS).filter(([, p]) => p),
		);
		expect(fromValidate).toEqual(expected);
	});

	it('groups every permission exactly once', () => {
		const grouped = Object.values(PERMISSION_GROUPS).flat();
		expect([...grouped].sort()).toEqual([...PERMISSIONS].sort());
	});
});

describe('validate.mjs permission lint', () => {
	it('reports a used call whose permission is missing', () => {
		const result = lint({ ...base, permissions: [] }, 'tmgr.tasks.list({});');
		expect(result.ok).toBe(true);
		expect(result.warnings).toEqual([
			'uses tmgr.tasks.list but does not declare the "tasks:read" permission',
		]);
	});

	it('reports a declared permission nothing uses', () => {
		const result = lint(
			{ ...base, permissions: ['tasks:read', 'comments:write'] },
			'tmgr.tasks.list({});',
		);
		expect(result.warnings).toEqual([
			'declares the "comments:write" permission but nothing in the plugin seems to use it; remove it to ask for less',
		]);
	});

	it('needs the event permission and counts page calls as use', () => {
		const result = lint(
			{
				...base,
				permissions: ['comments:read'],
				contributes: { views: [{ id: 'p', title: 'P', ui: 'ui/p.html' }] },
			},
			"tmgr.events.on('task.created', () => {});",
			{ 'p.html': "<script>tmgr.call('comments.list', {})</script>" },
		);
		expect(result.warnings).toEqual([
			'listens to task.created but does not declare the "tasks:read" permission',
		]);
	});

	it('does not claim a permission is unused when calls are dynamic', () => {
		const result = lint(
			{ ...base, permissions: ['tasks:read'] },
			"const name = 'list'; tmgr.tasks[name]({});\nconst api = tmgr;",
		);
		expect(result.warnings).toHaveLength(1);
		expect(result.warnings[0]).toMatch(/cannot be checked statically/);
	});

	it('groups the declared permissions for the report', () => {
		const result = lint(
			{ ...base, permissions: ['tasks:read', 'notifications'] },
			"tmgr.tasks.list({}); tmgr.ui.notify('x');",
		);
		expect(result.permissions).toEqual([
			{ group: 'Reads your data', permissions: ['tasks:read'] },
			{ group: 'Reaches outside the app', permissions: ['notifications'] },
		]);
	});

	it('treats page tmgr.notify as the notifications permission', () => {
		const result = lint(
			{
				...base,
				permissions: [],
				contributes: { views: [{ id: 'p', title: 'P', ui: 'ui/p.html' }] },
			},
			'',
			{ 'p.html': "<script>tmgr.notify('hi')</script>" },
		);
		expect(result.warnings).toEqual([
			'uses tmgr.notify but does not declare the "notifications" permission',
		]);
	});

	it.each([
		'const api = window.tmgr;',
		'const { tasks } = tmgr\ntasks.list({});',
		'run(tmgr);',
		'const t = tmgr?.tasks;',
	])('stays quiet about unused permissions for %s', (main) => {
		const result = lint({ ...base, permissions: ['tasks:read'] }, main);
		expect(result.warnings).toHaveLength(1);
		expect(result.warnings[0]).toMatch(/cannot be checked statically/);
	});

	it('ignores commented-out calls and longer method names', () => {
		const result = lint(
			{ ...base, permissions: ['pages:read'] },
			'// tmgr.tasks.list()\ntmgr.pageData.getMany(1, []);',
		);
		expect(result.warnings).toEqual([]);
	});

	it('exits 1 on warnings only with --strict', () => {
		const dir = mkdtempSync(join(tmpdir(), 'tmgr-lint-'));
		writeFileSync(
			join(dir, 'manifest.json'),
			JSON.stringify({ ...base, permissions: ['tasks:read'] }),
		);
		writeFileSync(join(dir, 'main.js'), '');
		expect(() =>
			execFileSync('node', [validatePath, dir], { stdio: 'pipe' }),
		).not.toThrow();
		expect(() =>
			execFileSync('node', [validatePath, dir, '--strict'], { stdio: 'pipe' }),
		).toThrow();
	});
});
