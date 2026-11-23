/**
 * @jest-environment jsdom
 */
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createApp, h, nextTick, type Component } from 'vue';
import { createI18n } from 'vue-i18n';
import manager from '../../locales/en/manager.json';
import {
	createRemotePolicyStore,
	type RemotePolicyApi,
} from '../remotePolicyStore';
import type { ManagerMachine, RemotePolicyInfo } from '../types';

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

const loadComponent = (): Component => {
	const filename = join(__dirname, '..', 'components', 'RemotePolicy.vue');
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const script = compileScript(descriptor, { id: 'rp' });
	const exports = run(toCjs(script.content), (id) => require(id));
	const comp = (exports.default ?? exports) as Component & { render?: unknown };
	const tpl = compileTemplate({
		source: descriptor.template!.content,
		filename,
		id: 'rp',
		compilerOptions: { mode: 'module' },
	});
	comp.render = run(toCjs(tpl.code), (id) => require(id)).render;
	return comp;
};

const i18n = createI18n({
	legacy: true,
	locale: 'en',
	messages: { en: { manager } },
});

const mount = (props: Record<string, unknown>) => {
	const el = document.createElement('div');
	document.body.appendChild(el);
	const app = createApp({ render: () => h(loadComponent(), props) }).use(i18n);
	app.mount(el);
	return { el, unmount: () => (app.unmount(), el.remove()) };
};

const A = 'mx_0123456789abcdef';
const B = 'mx_fedcba9876543210';
const machines: ManagerMachine[] = [
	{ machine: A, label: 'Studio', role: 'top' },
];
const info = (a: boolean, b: boolean): RemotePolicyInfo => ({
	legacy: false,
	leaders: [
		{ machine: A, ask: a, ops: ['list'] },
		{ machine: B, ask: b, ops: ['list'] },
	],
});

describe('RemotePolicy toggle', () => {
	it('shows one toggle per leader, named by label or machine id', () => {
		const { el, unmount } = mount({
			leaders: info(true, false).leaders,
			machines,
		});
		const text = el.textContent ?? '';
		expect(text).toContain('Allow asks from Studio');
		expect(text).toContain(`Allow asks from ${B}`);
		const boxes = el.querySelectorAll('input[type=checkbox]');
		expect(boxes).toHaveLength(2);
		expect((boxes[0] as HTMLInputElement).checked).toBe(true);
		expect((boxes[1] as HTMLInputElement).checked).toBe(false);
		unmount();
	});

	it('emits the leader and the new state when toggled', () => {
		const onToggle = jest.fn();
		const { el, unmount } = mount({
			leaders: info(false, false).leaders,
			machines,
			onToggle,
		});
		const box = el.querySelectorAll('input')[0] as HTMLInputElement;
		box.checked = true;
		box.dispatchEvent(new Event('change'));
		expect(onToggle).toHaveBeenCalledWith(A, true);
		unmount();
	});

	it('renders nothing without leaders and shows an error', async () => {
		const empty = mount({ leaders: [] });
		expect(empty.el.querySelector('input')).toBeNull();
		empty.unmount();
		const failing = mount({
			leaders: info(true, true).leaders,
			error: 'forbidden: inside a session',
		});
		await nextTick();
		expect(failing.el.textContent).toContain(
			'Could not change the setting: forbidden: inside a session',
		);
		failing.unmount();
	});
});

describe('remote policy store', () => {
	const setup = (set?: RemotePolicyApi['set']) => {
		const api: RemotePolicyApi = {
			policy: jest.fn(async () => info(false, false)),
			set: set ?? jest.fn(async (_l: string, ask: boolean) => info(ask, false)),
		};
		return { api, store: createRemotePolicyStore(api) };
	};

	it('loads leaders and applies the answer of a set', async () => {
		const { api, store } = setup();
		await store.refresh();
		expect(store.leaders.map((l) => l.ask)).toEqual([false, false]);
		await store.setAsk(A, true);
		expect(api.set).toHaveBeenCalledWith(A, true);
		expect(store.leaders.map((l) => l.ask)).toEqual([true, false]);
		expect(store.error).toBe('');
	});

	it('keeps the refusal text and reloads the real state when a set fails', async () => {
		const { api, store } = setup(async () => {
			throw new Error('forbidden: inside a session');
		});
		await store.setAsk(A, true);
		expect(store.error).toBe('forbidden: inside a session');
		expect(api.policy).toHaveBeenCalledTimes(1);
		expect(store.busy).toBe(false);
	});

	it('stays empty when the runner is too old', async () => {
		const { api, store } = setup();
		(api.policy as jest.Mock).mockResolvedValueOnce(null);
		await store.refresh();
		expect(store.leaders).toEqual([]);
	});
});
