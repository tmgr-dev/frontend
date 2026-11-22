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
import type { SpawnCardWire } from '../types';

const mockApi = {
	spawnCard: jest.fn(),
	spawnDecide: jest.fn(),
	runnerCaps: jest.fn(),
};
jest.mock('../api', () => ({
	spawnCard: (...a: unknown[]) => mockApi.spawnCard(...a),
	spawnDecide: (...a: unknown[]) => mockApi.spawnDecide(...a),
	runnerCaps: (...a: unknown[]) => mockApi.runnerCaps(...a),
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

const CARD = 'card_0123456789abcdef0123456789abcdef';
const REQUEST = JSON.stringify({
	v: 1,
	profile: 'claude',
	cwd: '/work/app',
	mode: 'ask',
	task: 'TM-1',
	target_id: 'mx_a',
});
const wire: SpawnCardWire = {
	summary: 's',
	request: REQUEST,
	nonce: 'n',
	exp_ms: Date.now() + 600_000,
	target_id: 'mx_a',
	target_fp: 'fp',
};

const flush = async () => {
	for (let i = 0; i < 6; i++) await Promise.resolve();
	await nextTick();
};

const mount = async () => {
	const el = document.createElement('div');
	document.body.appendChild(el);
	const app = createApp({
		render: () => h(load('SpawnApprovalCard'), { cardId: CARD, spawn: wire }),
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

const buttons = (el: HTMLElement) =>
	Array.from(el.querySelectorAll<HTMLButtonElement>('.sp-actions button'));

describe('SpawnApprovalCard actions', () => {
	beforeEach(() => {
		jest.resetAllMocks();
		mockApi.runnerCaps.mockResolvedValue(['spawn_decide.v1']);
		mockApi.spawnCard.mockResolvedValue({ state: 'issued' });
	});

	it('shows enabled Approve, Edit and Reject for a live issued card', async () => {
		const { el, unmount } = await mount();
		expect(buttons(el).map((b) => b.textContent?.trim())).toEqual([
			'Approve',
			'Edit',
			'Reject',
		]);
		expect(buttons(el).every((b) => !b.disabled)).toBe(true);
		expect(el.querySelector('.sp-outdated')).toBeNull();
		unmount();
	});

	it('hides the buttons once the card is no longer issued', async () => {
		mockApi.spawnCard.mockResolvedValue({ state: 'claimed' });
		const { el, unmount } = await mount();
		expect(buttons(el)).toHaveLength(0);
		unmount();
	});

	it('disables the buttons with a hint when the runner lacks the capability', async () => {
		mockApi.runnerCaps.mockResolvedValue(['chat.v1']);
		const { el, unmount } = await mount();
		expect(buttons(el).every((b) => b.disabled)).toBe(true);
		expect(el.querySelector('.sp-outdated')?.textContent).toContain(
			'Update tmgr CLI to approve from desktop',
		);
		unmount();
	});

	it('learns the runner is outdated from the runner_outdated error', async () => {
		mockApi.spawnDecide.mockRejectedValue('runner_outdated');
		const { el, unmount } = await mount();
		buttons(el)[0].click();
		await flush();
		expect(buttons(el).every((b) => b.disabled)).toBe(true);
		expect(el.querySelector('.sp-outdated')).not.toBeNull();
		unmount();
	});

	it('approve sends the exact request bytes and shows only the polled state', async () => {
		let release!: (v: unknown) => void;
		mockApi.spawnDecide.mockReturnValue(new Promise((r) => (release = r)));
		const { el, unmount } = await mount();
		buttons(el)[0].click();
		await flush();
		expect(mockApi.spawnDecide).toHaveBeenCalledWith(
			CARD,
			REQUEST,
			'approve',
			undefined,
		);
		expect(buttons(el).every((b) => b.disabled)).toBe(true);
		release({ state: 'approved' });
		await flush();
		expect(el.querySelector('.sp-status')?.textContent).not.toContain(
			'Approved',
		);
		expect(el.querySelector('.pill')?.textContent).toContain('Waiting');
		unmount();
	});

	it('reject shows the runner-reported refusal', async () => {
		mockApi.spawnDecide.mockResolvedValue({ state: 'refused' });
		const { el, unmount } = await mount();
		mockApi.spawnCard.mockResolvedValue({
			state: 'refused',
			reason: 'rejected',
		});
		buttons(el)[2].click();
		await flush();
		expect(mockApi.spawnDecide.mock.calls[0][2]).toBe('reject');
		expect(el.querySelector('.sp-resolved')?.textContent).toContain('Rejected');
		unmount();
	});

	it('shows decide errors in the error note', async () => {
		mockApi.spawnDecide.mockRejectedValue('hash mismatch');
		const { el, unmount } = await mount();
		buttons(el)[0].click();
		await flush();
		expect(el.querySelector('.dc-note.dc-err')?.textContent).toContain(
			'hash mismatch',
		);
		unmount();
	});

	it('edit opens a prefilled form and submits the edited payload', async () => {
		mockApi.spawnDecide.mockResolvedValue({
			state: 'refused',
			card_id: 'card_ffffffffffffffffffffffffffffffff',
		});
		const { el, unmount } = await mount();
		buttons(el)[1].click();
		await flush();
		const cwd = el.querySelector<HTMLInputElement>(
			'.sp-edit input[name="cwd"]',
		)!;
		const task = el.querySelector<HTMLInputElement>(
			'.sp-edit input[name="task"]',
		)!;
		const mode = el.querySelector<HTMLSelectElement>(
			'.sp-edit select[name="mode"]',
		)!;
		expect(cwd.value).toBe('/work/app');
		expect(task.value).toBe('TM-1');
		expect(mode.value).toBe('ask');
		cwd.value = '/work/other';
		cwd.dispatchEvent(new Event('input'));
		mode.value = 'accept-edits';
		mode.dispatchEvent(new Event('change'));
		el.querySelector<HTMLButtonElement>(
			'.sp-edit button[type="submit"]',
		)!.click();
		await flush();
		expect(mockApi.spawnDecide).toHaveBeenCalledWith(CARD, REQUEST, 'edit', {
			cwd: '/work/other',
			mode: 'accept-edits',
			task: 'TM-1',
		});
		expect(el.textContent).toContain('Replaced by a new card');
		unmount();
	});

	it('cancel closes the edit form without a call', async () => {
		const { el, unmount } = await mount();
		buttons(el)[1].click();
		await flush();
		el.querySelector<HTMLButtonElement>(
			'.sp-edit button[type="button"]',
		)!.click();
		await flush();
		expect(el.querySelector('.sp-edit')).toBeNull();
		expect(mockApi.spawnDecide).not.toHaveBeenCalled();
		unmount();
	});
});
