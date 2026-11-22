/**
 * @jest-environment jsdom
 */
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createApp, createSSRApp, h, type Component } from 'vue';
import { createI18n } from 'vue-i18n';
import { renderToString } from 'vue/server-renderer';
import manager from '../../locales/en/manager.json';
import type { ManagerMachine, RoomInfo } from '../types';

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
	comp.render = run(toCjs(tpl.code), localRequire).render;
	cache.set(name, comp);
	return comp;
};

const localRequire = (id: string): unknown => {
	if (id.endsWith('.vue'))
		return load(id.replace(/^.*\//, '').replace('.vue', ''));
	if (id === './viewHelpers') return require('../components/viewHelpers');
	if (id === '../roomOrder') return require('../roomOrder');
	if (id === '../machineTree') return require('../machineTree');
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
		createSSRApp({ render: () => h(load(name), props) }).use(i18n),
	);

const mount = (name: string, props: Record<string, unknown>) => {
	const el = document.createElement('div');
	document.body.appendChild(el);
	const app = createApp({ render: () => h(load(name), props) }).use(i18n);
	app.mount(el);
	return {
		el,
		unmount: () => {
			app.unmount();
			el.remove();
		},
	};
};

describe('GateBlockersBanner', () => {
	it('lists the blocking devices', async () => {
		const html = await render('GateBlockersBanner', {
			labels: ['iPhone', 'Studio Mac'],
		});
		expect(html).toContain('Update these devices first: iPhone, Studio Mac');
		expect(html).toContain('role="alert"');
	});

	it('renders nothing without blockers', async () => {
		const html = await render('GateBlockersBanner', { labels: [] });
		expect(html).not.toContain('Update these devices');
	});
});

describe('RunnerAddedNotice', () => {
	it('names the machine and emits revoke and dismiss', async () => {
		const html = await render('RunnerAddedNotice', { label: 'Studio' });
		expect(html).toContain('Runner on Studio was added to chat');
		const onRevoke = jest.fn();
		const onDismiss = jest.fn();
		const { el, unmount } = mount('RunnerAddedNotice', {
			label: 'Studio',
			onRevoke,
			onDismiss,
		});
		const buttons = el.querySelectorAll('button');
		(buttons[0] as HTMLButtonElement).click();
		(buttons[1] as HTMLButtonElement).click();
		expect(onRevoke).toHaveBeenCalledTimes(1);
		expect(onDismiss).toHaveBeenCalledTimes(1);
		unmount();
	});
});

const room = (machine: string, kind: 'manager' | 'session', title: string) =>
	({
		room: { kind, machine, ...(kind === 'session' ? { session: title } : {}) },
		title,
	}) as RoomInfo;

const machines: ManagerMachine[] = [
	{ machine: 'mx_top', label: 'Studio', role: 'top', local: true },
	{ machine: 'mx_sub', label: 'Server', role: 'sub' },
];

describe('ChatList machine tree', () => {
	const rooms = [
		room('mx_sub', 'session', 'sub-sess'),
		room('mx_top', 'session', 'top-sess'),
		room('mx_sub', 'manager', 'Sub manager'),
		room('mx_top', 'manager', 'Top manager'),
	];

	it('groups rooms under Main and Sub-manager headers', async () => {
		const html = await render('ChatList', { rooms, machines });
		const at = (s: string) => html.indexOf(s);
		expect(html).toContain('Main · Studio');
		expect(html).toContain('Server · Sub-manager');
		expect(at('Main · Studio')).toBeLessThan(at('Top manager'));
		expect(at('Top manager')).toBeLessThan(at('top-sess'));
		expect(at('top-sess')).toBeLessThan(at('Server · Sub-manager'));
		expect(at('Server · Sub-manager')).toBeLessThan(at('Sub manager'));
		expect(at('Sub manager')).toBeLessThan(at('sub-sess'));
	});

	it('falls back to "This computer" for an unnamed main machine', async () => {
		const html = await render('ChatList', {
			rooms,
			machines: [{ ...machines[0], label: null }, machines[1]],
		});
		expect(html).toContain('Main · This computer');
	});

	it('renders flat, with no machine headers, for a single machine', async () => {
		const html = await render('ChatList', {
			rooms: rooms.filter((r) => r.room.machine === 'mx_top'),
			machines,
		});
		expect(html).not.toContain('Main ·');
		expect(html).toContain('Pinned');
	});

	it('renders flat without machine info', async () => {
		const html = await render('ChatList', { rooms, machines: [] });
		expect(html).not.toContain('Sub-manager');
		expect(html).toContain('Pinned');
	});
});
