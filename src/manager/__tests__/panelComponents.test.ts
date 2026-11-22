import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createSSRApp, type Component } from 'vue';
import { createI18n } from 'vue-i18n';
import { renderToString } from 'vue/server-renderer';
import manager from '../../locales/en/manager.json';
import type {
	ChatMessage,
	RoomInfo,
	SpawnRequest,
	TrustDevice,
} from '../types';

const dir = join(__dirname, '..', 'components');
const cache = new Map<string, Component>();

const toCjs = (code: string): string =>
	ts.transpileModule(code, {
		compilerOptions: {
			module: ts.ModuleKind.CommonJS,
			target: ts.ScriptTarget.ES2020,
			esModuleInterop: true,
		},
	}).outputText;

const run = (code: string, requireFn: (id: string) => unknown) => {
	const module = { exports: {} as Record<string, unknown> };
	new Function('module', 'exports', 'require', code)(
		module,
		module.exports,
		requireFn,
	);
	return module.exports;
};

const load = (name: string): Component => {
	const hit = cache.get(name);
	if (hit) return hit;
	const filename = join(dir, `${name}.vue`);
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const script = compileScript(descriptor, { id: name });
	const scriptExports = run(toCjs(script.content), localRequire);
	const comp = (scriptExports.default ?? scriptExports) as Component & {
		render?: unknown;
	};
	const tpl = compileTemplate({
		source: descriptor.template!.content,
		filename,
		id: name,
		compilerOptions: { mode: 'module' },
	});
	const tplExports = run(toCjs(tpl.code), localRequire);
	comp.render = tplExports.render;
	cache.set(name, comp);
	return comp;
};

const localRequire = (id: string): unknown => {
	if (id.endsWith('.vue'))
		return load(id.replace(/^.*\//, '').replace('.vue', ''));
	if (id === './viewHelpers') return require('../components/viewHelpers');
	if (id === './trust/trustHelpers')
		return require('../components/trust/trustHelpers');
	if (id === '../types') return require('../types');
	return require(id);
};

const i18n = createI18n({
	legacy: true,
	locale: 'en',
	messages: { en: { manager } },
});

const render = (name: string, props: Record<string, unknown>) =>
	renderToString(
		createSSRApp({
			render: () => require('vue').h(load(name), props),
		}).use(i18n),
	);

const T0 = new Date(2026, 10, 21, 9, 15).getTime();
const mgrRoom = { kind: 'manager' as const, machine: 'mac' };
const sess = (session: string, machine = 'mac') => ({
	kind: 'session' as const,
	machine,
	session,
});

const msg = (id: string, body: string): ChatMessage => ({
	v: 1,
	id,
	room: sess('s1'),
	author: { kind: 'session', id: 's1', agent: 'claude' },
	ts_ms: T0,
	kind: 'text',
	body_md: body,
});

const rooms: RoomInfo[] = [
	{
		room: sess('s1'),
		title: 'claude · TM-1021',
		agent: 'claude',
		state: 'working',
		context_pct: 71,
		context_tokens: 142_000,
	},
	{ room: sess('s2', 'studio'), title: 'codex · TM-9', agent: 'codex' },
	{ room: mgrRoom, title: 'Manager' },
];

const devices: TrustDevice[] = [
	{
		id: 'a',
		kind: 'desktop',
		label: 'Mac',
		fp: 'ab12cd34ef56ab12ff',
		state: 'trusted',
		is_self: true,
	},
	{
		id: 'b',
		kind: 'mobile',
		label: 'Old phone',
		fp: '1111222233334444',
		state: 'revoked',
		is_self: false,
	},
];

describe('manager detail panel components', () => {
	it('SessionTab shows agent, state, context, machine and last messages', async () => {
		const html = await render('SessionTab', {
			info: rooms[0],
			messages: [msg('a', 'Running **tests**'), msg('b', 'All green')],
		});
		expect(html).toContain('claude · TM-1021');
		expect(html).toContain('Working');
		expect(html).toContain('142k / 200k · 71%');
		expect(html).toContain('>mac<');
		expect(html).toContain('Running tests');
		expect(html).toContain('All green');
		expect(html).not.toContain('Stop');
		expect(html).not.toContain('Hand off');
	});

	it('SessionTab suggests a handoff above 180k context', async () => {
		const html = await render('SessionTab', {
			info: { ...rooms[0], context_tokens: 190_000, context_pct: 95 },
		});
		expect(html).toContain('Hand off to a fresh session');
	});

	it('SessionTab asks to pick a session for the Manager room', async () => {
		const html = await render('SessionTab', { info: rooms[2] });
		expect(html).toContain('Select a session');
	});

	it('HierarchyTab lists the Manager first, then sessions per machine', async () => {
		const html = await render('HierarchyTab', {
			rooms,
			machines: [
				{ machine: 'mac', role: 'top' },
				{ machine: 'studio', role: 'sub' },
			],
			activeKey: 'session:mac:s1',
		});
		expect(html.indexOf('Manager')).toBeLessThan(html.indexOf('claude'));
		expect(html.indexOf('Controls its own machine directly')).toBeLessThan(
			html.indexOf('>studio<'),
		);
		expect(html).toContain('Main · mac · asks you to approve risky actions');
		expect(html).toContain('<b>codex</b>');
		expect(html).toContain('hs st-working on');
	});

	it('HierarchyTab shows a sub-manager with its local scope banner and task ids', async () => {
		const html = await render('HierarchyTab', {
			rooms: [
				...rooms,
				{
					room: { kind: 'manager', machine: 'studio' },
					title: 'Studio manager',
				},
				{ ...rooms[1], task: 'TM-1014' },
			],
			machines: [
				{ machine: 'mac', role: 'top' },
				{ machine: 'studio', role: 'sub', label: 'Studio' },
			],
			activeKey: null,
		});
		expect(html).toContain('Studio manager');
		expect(html).toContain(
			'Sub-manager · Studio · takes orders from the main Manager',
		);
		expect(html).toContain('Local scope: acts only on Studio.');
		expect(html).toContain('TM-1014');
	});

	it('AuditTab shows the empty state and entries', async () => {
		expect(await render('AuditTab', { entries: [] })).toContain(
			'Nothing recorded yet',
		);
		const html = await render('AuditTab', {
			entries: [
				{
					ts_ms: T0,
					action: 'spawn',
					detail: 'claude on mac',
					needs_approval: true,
				},
			],
		});
		expect(html).toContain('09:15');
		expect(html).toContain('claude on mac');
		expect(html).toContain('needs approval');
	});

	it('AuditTab surfaces a load error', async () => {
		const html = await render('AuditTab', { entries: [], error: 'boom' });
		expect(html).toContain('role="alert"');
		expect(html).toContain('boom');
		expect(html).not.toContain('Nothing recorded yet');
	});

	it('EncryptionTab shows grouped fingerprint, device states and explanation', async () => {
		const html = await render('EncryptionTab', {
			selfFp: 'ab12cd34ef56ab12ff',
			devices,
		});
		expect(html).toContain('End-to-end encrypted');
		expect(html).toContain('ab12 cd34 ef56 ab12');
		expect(html).toContain('this device');
		expect(html).toContain('Trusted');
		expect(html).toContain('Revoked');
		expect(html).toContain('Manage devices');
		expect(html).toContain('What the AI provider sees');
	});

	it('DetailPanel renders four tabs and the selected one', async () => {
		const html = await render('DetailPanel', {
			tab: 'hierarchy',
			rooms,
			activeKey: null,
		});
		for (const t of ['Session', 'Hierarchy', 'Audit log', 'Encryption'])
			expect(html).toContain(t);
		expect(html).toContain('aria-selected="true"');
		expect(html).toContain('class="hier"');
	});
});

describe('manager dialogs', () => {
	const form: SpawnRequest = {
		agent: 'claude',
		machine: 'mac',
		folder: '',
		name: '',
		task: '',
		prompt: '',
	};

	it('NewSessionDialog offers agents and machines, no safety toggles', async () => {
		const html = await render('NewSessionDialog', {
			form,
			machines: ['mac', 'studio'],
		});
		for (const a of ['claude', 'codex', 'opencode', 'agy'])
			expect(html).toContain(`>${a}<`);
		expect(html).toContain('>studio<');
		expect(html).toContain('First prompt');
		expect(html).not.toMatch(/sandbox|bypass/i);
		expect(html).toContain('disabled');
	});

	it('NewSessionDialog enables start with a folder and shows the error', async () => {
		const html = await render('NewSessionDialog', {
			form: { ...form, folder: '~/app' },
			machines: ['mac'],
			error: 'not available yet',
		});
		expect(html).not.toContain('disabled');
		expect(html).toContain('not available yet');
	});

	it('SwitchAgentDialog shows targets, models, CLI-only note and error', async () => {
		const html = await render('SwitchAgentDialog', {
			agent: 'agy',
			model: 'default',
			models: ['default'],
			current: 'claude',
			error: 'Use the tmgr CLI.',
		});
		for (const a of ['claude', 'codex', 'opencode', 'agy'])
			expect(html).toContain(`<b>${a}</b>`);
		expect(html).toContain('Running now');
		expect(html).toContain('managed from the tmgr CLI by design');
		expect(html).toContain('Switch to agy · default');
		expect(html).toContain('Use the tmgr CLI.');
	});

	it('SwitchAgentDialog shows list labels and keeps the slug in the submit text', async () => {
		const html = await render('SwitchAgentDialog', {
			agent: 'agy',
			model: 'gemini-3.8-flash-medium',
			models: ['default', 'gemini-3.8-flash-medium'],
			labels: { 'gemini-3.8-flash-medium': 'Gemini 3.8 Flash (Medium)' },
			current: 'claude',
		});
		expect(html).toContain('Gemini 3.8 Flash (Medium)');
		expect(html).toContain('Switch to agy · gemini-3.8-flash-medium');
	});

	it('SwitchAgentDialog shows claude model labels and keeps the alias in the submit text', async () => {
		const html = await render('SwitchAgentDialog', {
			agent: 'claude',
			model: 'fable',
			models: ['opus', 'sonnet', 'haiku', 'fable'],
			current: 'claude',
			currentModel: 'opus',
		});
		for (const label of ['Opus 5.5', 'Sonnet 5.5', 'Haiku 5.5', 'Fable 5.2'])
			expect(html).toContain(label);
		expect(html).toContain('Switch to claude · fable');
	});
});
