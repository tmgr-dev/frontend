/**
 * @jest-environment jsdom
 */
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createApp, h, nextTick, reactive, type Component } from 'vue';
import { createI18n } from 'vue-i18n';
import manager from '../../locales/en/manager.json';
import type { PollWire } from '../types';

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

const CARD = 'poll_abc';
const q = (over: Record<string, unknown> = {}) => ({
	header: 'DB',
	question: 'Which DB?',
	options: [
		{ label: 'Postgres', description: 'Relational', recommended: true },
		{ label: 'SQLite' },
		{ label: 'MySQL' },
	],
	...over,
});
const poll = (over: Partial<PollWire> = {}): PollWire => ({
	card_id: CARD,
	session: 's_1',
	questions: [q()],
	...over,
});

const flush = async () => {
	for (let i = 0; i < 6; i++) await Promise.resolve();
	await nextTick();
};

const mount = async (
	card: PollWire,
	props: {
		closed?: { closed: string; picked?: string; why?: string };
		answerable?: boolean;
	} = {},
) => {
	const el = document.createElement('div');
	document.body.appendChild(el);
	const state = reactive({ closed: props.closed });
	const app = createApp({
		render: () =>
			h(load('PollCard'), {
				poll: card,
				answerable: props.answerable ?? true,
				closed: state.closed,
			}),
	}).use(i18n);
	app.mount(el);
	await flush();
	return {
		el,
		close: async (c: { closed: string; picked?: string; why?: string }) => {
			state.closed = c;
			await flush();
		},
		unmount: () => {
			app.unmount();
			el.remove();
		},
	};
};

const opts = (el: HTMLElement) =>
	Array.from(el.querySelectorAll<HTMLButtonElement>('button.po-opt'));
const click = async (btn: Element | null) => {
	(btn as HTMLButtonElement).click();
	await flush();
};
const type = async (el: HTMLElement, index: number, text: string) => {
	const input = el.querySelectorAll<HTMLInputElement>('.po-other-input')[index];
	input.value = text;
	input.dispatchEvent(new Event('input'));
	await flush();
};

describe('PollCard', () => {
	beforeEach(() => {
		jest.resetAllMocks();
		jest.useRealTimers();
		mockInvoke.mockResolvedValue(undefined);
	});

	it('shows the chip, question, descriptions and the Recommended badge', async () => {
		const { el, unmount } = await mount(poll());
		expect(el.querySelector('.po-chip')?.textContent).toBe('DB');
		expect(el.textContent).toContain('Which DB?');
		expect(el.textContent).toContain('Relational');
		expect(el.querySelectorAll('.po-badge')).toHaveLength(1);
		expect(opts(el)[0].textContent).toContain('Recommended');
		unmount();
	});

	it('a single-select tap answers at once with the exact invoke args', async () => {
		const { el, unmount } = await mount(poll());
		await click(opts(el)[1]);
		expect(mockInvoke).toHaveBeenCalledTimes(1);
		expect(mockInvoke).toHaveBeenCalledWith('manager_poll_answer', {
			cardId: CARD,
			answers: [{ question_idx: 0, choices: [1] }],
		});
		expect(el.querySelector('.po-sending')).not.toBeNull();
		expect(el.querySelector('.po-send')).toBeNull();
		unmount();
	});

	it('multi select toggles and sends only on Send answer', async () => {
		const { el, unmount } = await mount(
			poll({ questions: [q({ multi_select: true })] }),
		);
		expect(el.querySelector('.po-other-input')).toBeNull();
		const send = () => el.querySelector<HTMLButtonElement>('.po-send')!;
		expect(send().disabled).toBe(true);
		await click(opts(el)[2]);
		await click(opts(el)[0]);
		await click(opts(el)[2]);
		await click(opts(el)[1]);
		expect(mockInvoke).not.toHaveBeenCalled();
		expect(opts(el)[2].getAttribute('aria-pressed')).toBe('false');
		await click(send());
		expect(mockInvoke).toHaveBeenCalledWith('manager_poll_answer', {
			cardId: CARD,
			answers: [{ question_idx: 0, choices: [0, 1] }],
		});
		unmount();
	});

	it('Other text on a single-select sends the text, not a choice', async () => {
		const { el, unmount } = await mount(poll());
		expect(
			el.querySelector<HTMLButtonElement>('.po-other-send')!.disabled,
		).toBe(true);
		await type(el, 0, '  MariaDB ');
		await click(el.querySelector('.po-other-send'));
		expect(mockInvoke).toHaveBeenCalledWith('manager_poll_answer', {
			cardId: CARD,
			answers: [{ question_idx: 0, choices: [], other: 'MariaDB' }],
		});
		unmount();
	});

	it('several questions are answered in one go with one Send answer button', async () => {
		const { el, unmount } = await mount(
			poll({
				questions: [q(), q({ question: 'Second?', multi_select: true })],
			}),
		);
		const send = () => el.querySelector<HTMLButtonElement>('.po-send')!;
		const buttons = () => opts(el);
		await click(buttons()[2]);
		expect(mockInvoke).not.toHaveBeenCalled();
		expect(send().disabled).toBe(true);
		await click(buttons()[3]);
		await click(buttons()[5]);
		expect(send().disabled).toBe(false);
		await click(send());
		expect(mockInvoke).toHaveBeenCalledTimes(1);
		expect(mockInvoke).toHaveBeenCalledWith('manager_poll_answer', {
			cardId: CARD,
			answers: [
				{ question_idx: 0, choices: [2] },
				{ question_idx: 1, choices: [0, 2] },
			],
		});
		unmount();
	});

	it('typing Other clears the tapped choice and tapping a choice clears the text', async () => {
		const { el, unmount } = await mount(
			poll({ questions: [q(), q({ question: 'Second?' })] }),
		);
		await click(opts(el)[0]);
		await type(el, 0, 'mine');
		expect(opts(el)[0].getAttribute('aria-pressed')).toBe('false');
		await click(opts(el)[1]);
		expect(el.querySelector<HTMLInputElement>('.po-other-input')!.value).toBe(
			'',
		);
		unmount();
	});

	it('shows the failure and lets the user retry when the send is rejected', async () => {
		mockInvoke.mockRejectedValueOnce('Unknown or closed question');
		const { el, unmount } = await mount(poll());
		await click(opts(el)[0]);
		expect(el.querySelector('.dc-err')?.textContent).toContain(
			'Could not send the answer: Unknown or closed question',
		);
		expect(el.querySelector('.po-sending')).toBeNull();
		await click(opts(el)[0]);
		expect(mockInvoke).toHaveBeenCalledTimes(2);
		unmount();
	});

	it('ends the spinner after 10 s with the terminal hint and no buttons', async () => {
		jest.useFakeTimers();
		const { el, unmount } = await mount(poll());
		await click(opts(el)[0]);
		expect(el.querySelector('.po-sending')).not.toBeNull();
		jest.advanceTimersByTime(9_999);
		await flush();
		expect(el.querySelector('.po-sending')).not.toBeNull();
		jest.advanceTimersByTime(2);
		await flush();
		expect(el.querySelector('.po-sending')).toBeNull();
		expect(el.textContent).toContain('No response. Answer in the terminal.');
		expect(el.querySelectorAll('button')).toHaveLength(0);
		unmount();
	});

	it('the close message ends the in-flight state before the timeout', async () => {
		jest.useFakeTimers();
		const { el, close, unmount } = await mount(poll());
		await click(opts(el)[0]);
		await close({ closed: 'applied', picked: 'Postgres' });
		expect(el.querySelector('.po-sending')).toBeNull();
		expect(el.textContent).toContain('Answered: Postgres');
		jest.advanceTimersByTime(20_000);
		await flush();
		expect(el.textContent).not.toContain('No response');
		unmount();
	});

	it.each([
		[{ closed: 'applied', picked: 'SQLite' }, 'Answered: SQLite'],
		[{ closed: 'stale' }, 'The question changed or was answered elsewhere'],
		[
			{ closed: 'failed', why: 'the screen changed' },
			'Could not apply the answer: the screen changed',
		],
		[{ closed: 'terminal' }, 'Answered in the terminal'],
		[{ closed: 'gone' }, 'The session ended'],
	])('a closed card %j reads %s and has no buttons', async (closed, text) => {
		const { el, unmount } = await mount(poll(), { closed });
		expect(el.querySelector('.po-closed')?.textContent).toBe(text);
		expect(el.querySelectorAll('button')).toHaveLength(0);
		expect(el.querySelector('input')).toBeNull();
		unmount();
	});

	it('strips bidi and format controls from every text', async () => {
		const { el, unmount } = await mount(
			poll({
				questions: [
					q({
						header: 'D‮B',
						question: 'Wh‮ich?',
						options: [{ label: 'Pos‮tgres', description: 'Rel‮ational' }],
					}),
				],
			}),
			{ closed: { closed: 'applied', picked: 'Po‮stgres' } },
		);
		expect(el.textContent).not.toMatch(/[‎‏‪-‮⁦-⁩]/);
		expect(el.textContent).toContain('Which?');
		expect(el.textContent).toContain('Answered: Postgres');
		unmount();
	});

	it('can_answer false shows no buttons, the terminal hint and why_not', async () => {
		const { el, unmount } = await mount(
			poll({ can_answer: false, why_not: 'The picker is not on screen' }),
		);
		expect(el.querySelectorAll('button')).toHaveLength(0);
		expect(el.querySelector('input')).toBeNull();
		expect(el.textContent).toContain('Answer in the terminal');
		expect(el.textContent).toContain('The picker is not on screen');
		unmount();
	});

	it('a room that cannot answer from the desktop shows the terminal hint', async () => {
		const { el, unmount } = await mount(poll(), { answerable: false });
		expect(el.querySelectorAll('button')).toHaveLength(0);
		expect(el.textContent).toContain('Answer in the terminal');
		unmount();
	});
});
