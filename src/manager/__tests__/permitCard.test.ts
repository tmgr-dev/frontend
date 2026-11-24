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
import type { PermitCardWire } from '../types';

const mockInvoke = jest.fn();
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...a: unknown[]) => mockInvoke(...a),
}));

const dir = join(__dirname, '..', 'components');

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

const localRequire = (id: string): unknown => {
	if (id.endsWith('.vue'))
		return load(id.replace(/^.*\//, '').replace('.vue', ''));
	if (id.startsWith('./')) return require(`../components/${id.slice(2)}`);
	return require(id);
};

const cache = new Map<string, Component>();
const load = (name: string): Component => {
	const hit = cache.get(name);
	if (hit) return hit;
	const filename = join(dir, `${name}.vue`);
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const script = compileScript(descriptor, { id: name });
	const exportsOf = run(toCjs(script.content), localRequire);
	const comp = (exportsOf.default ?? exportsOf) as Component & {
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

const i18n = createI18n({
	legacy: true,
	locale: 'en',
	messages: { en: { manager } },
});

const CARD = 'card-abc';
const permit = (over: Partial<PermitCardWire> = {}): PermitCardWire => ({
	session: 's_abcdefghij',
	agent: 'claude-code',
	tool: 'Bash',
	text: 'ls -la',
	path: '/work/app',
	always_label: 'Always allow ls',
	exp_ms: Date.now() + 60_000,
	desktop_ok: true,
	...over,
});

const flush = async () => {
	for (let i = 0; i < 6; i++) await Promise.resolve();
	await nextTick();
};

const mount = async (
	card: PermitCardWire,
	props: { closed?: string; answerable?: boolean } = {},
) => {
	const el = document.createElement('div');
	document.body.appendChild(el);
	const app = createApp({
		render: () =>
			h(load('PermitCard'), { cardId: CARD, permit: card, ...props }),
	}).use(i18n);
	app.mount(el);
	await flush();
	return {
		el,
		unmount: () => {
			app.unmount();
			el.remove();
		},
	};
};

const labels = (el: HTMLElement) =>
	Array.from(el.querySelectorAll('button')).map((b) => b.textContent?.trim());
const click = async (el: HTMLElement, selector: string) => {
	el.querySelector<HTMLButtonElement>(selector)!.click();
	await flush();
};

describe('PermitCard', () => {
	beforeEach(() => {
		jest.resetAllMocks();
		mockInvoke.mockResolvedValue(undefined);
	});

	it('shows the request summary', async () => {
		const { el, unmount } = await mount(permit());
		expect(el.textContent).toContain('ls -la');
		expect(el.textContent).toContain('/work/app');
		unmount();
	});

	it('has no buttons and shows the phone text when desktop_ok is false', async () => {
		const { el, unmount } = await mount(permit({ desktop_ok: false }));
		expect(labels(el)).toEqual([]);
		expect(el.textContent).toContain('Answer on your phone or in tmgr attach');
		unmount();
	});

	it('has no buttons when the room is not answerable from the desktop', async () => {
		const { el, unmount } = await mount(permit(), { answerable: false });
		expect(labels(el)).toEqual([]);
		expect(el.textContent).toContain('Answer on your phone or in tmgr attach');
		unmount();
	});

	it('answers a plain card with one tap and exact invoke args', async () => {
		const { el, unmount } = await mount(permit());
		await click(el, '.pm-once');
		expect(mockInvoke).toHaveBeenCalledTimes(1);
		expect(mockInvoke).toHaveBeenCalledWith('manager_permit_answer', {
			cardId: CARD,
			choice: 'once',
			confirmed: false,
		});
		expect(labels(el)).toEqual([]);
		unmount();
	});

	it('deny sends deny without a confirm', async () => {
		const { el, unmount } = await mount(permit({ danger: ['destructive'] }));
		await click(el, '.pm-deny');
		expect(mockInvoke).toHaveBeenCalledWith('manager_permit_answer', {
			cardId: CARD,
			choice: 'deny',
			confirmed: false,
		});
		unmount();
	});

	it('a danger card shows its warning in red and needs a second confirm tap', async () => {
		const { el, unmount } = await mount(
			permit({ warning: 'deletes files', danger: ['destructive'] }),
		);
		expect(el.querySelector('.pm-warn')?.textContent).toContain(
			'deletes files',
		);
		await click(el, '.pm-once');
		expect(mockInvoke).not.toHaveBeenCalled();
		expect(labels(el)).toEqual(['Confirm: allow once', 'Cancel']);
		await click(el, '.pm-confirm');
		expect(mockInvoke).toHaveBeenCalledWith('manager_permit_answer', {
			cardId: CARD,
			choice: 'once',
			confirmed: true,
		});
		unmount();
	});

	it('cancel leaves the confirm step without invoking', async () => {
		const { el, unmount } = await mount(permit({ warning: 'careful' }));
		await click(el, '.pm-once');
		await click(el, '.pm-cancel');
		expect(mockInvoke).not.toHaveBeenCalled();
		expect(labels(el)).toContain('Allow once');
		unmount();
	});

	it('always is never preselected and needs its own confirm on every card', async () => {
		const { el, unmount } = await mount(permit());
		expect(
			el.querySelector('[autofocus], .active, [aria-pressed=true]'),
		).toBeNull();
		expect(document.activeElement?.className ?? '').not.toContain('pm-always');
		await click(el, '.pm-always');
		expect(mockInvoke).not.toHaveBeenCalled();
		expect(labels(el)).toEqual(['Confirm: always allow', 'Cancel']);
		await click(el, '.pm-confirm');
		expect(mockInvoke).toHaveBeenCalledWith('manager_permit_answer', {
			cardId: CARD,
			choice: 'always',
			confirmed: true,
		});
		unmount();
	});

	it('does not offer always when the card has no always label', async () => {
		const { el, unmount } = await mount(permit({ always_label: undefined }));
		expect(el.querySelector('.pm-always')).toBeNull();
		unmount();
	});

	it('Enter answers nothing and is cancelled on the card', async () => {
		const { el, unmount } = await mount(permit());
		const card = el.querySelector('.dcard')!;
		const ev = new KeyboardEvent('keydown', {
			key: 'Enter',
			bubbles: true,
			cancelable: true,
		});
		el.querySelector('.pm-once')!.dispatchEvent(ev);
		await flush();
		expect(ev.defaultPrevented).toBe(true);
		expect(mockInvoke).not.toHaveBeenCalled();
		expect(card.querySelector('form')).toBeNull();
		unmount();
	});

	it.each([
		['answered on Phone', 'Answered on Phone'],
		['gone', 'Gone'],
		['answered in attach', 'Answered in tmgr attach'],
	])(
		'closed "%s" flips the card to "%s" and removes the buttons',
		async (reason, text) => {
			const { el, unmount } = await mount(permit(), { closed: reason });
			expect(labels(el)).toEqual([]);
			expect(el.querySelector('.pm-closed')?.textContent).toBe(text);
			unmount();
		},
	);

	it('shows the failure as an alert', async () => {
		mockInvoke.mockRejectedValue('This permission card has expired');
		const { el, unmount } = await mount(permit());
		await click(el, '.pm-once');
		expect(el.querySelector('[role=alert].dc-err')?.textContent).toContain(
			'This permission card has expired',
		);
		expect(labels(el)).toContain('Allow once');
		unmount();
	});
});
