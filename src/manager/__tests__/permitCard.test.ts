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
import { permitGroups } from '../permitCards';
import type { ChatMessage, PermitCardWire } from '../types';

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
	props: {
		closed?: string;
		answerable?: boolean;
		live?: boolean;
		selfFp?: string;
	} = {},
) => {
	const el = document.createElement('div');
	document.body.appendChild(el);
	const app = createApp({
		render: () =>
			h(load('PermitCard'), {
				cardId: CARD,
				permit: card,
				live: true,
				...props,
			}),
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

describe('PermitCard states', () => {
	beforeEach(() => {
		jest.resetAllMocks();
		jest.useRealTimers();
		mockInvoke.mockResolvedValue(undefined);
	});

	it('a card that is not in the book has no buttons and the phone line', async () => {
		const { el, unmount } = await mount(permit(), { live: false });
		expect(labels(el)).toEqual([]);
		expect(el.textContent).toContain('Answer on your phone or in tmgr attach');
		unmount();
	});

	it('a card already past exp_ms shows Expired with only a Refresh button', async () => {
		const { el, unmount } = await mount(permit({ exp_ms: Date.now() - 1 }));
		expect(labels(el)).toEqual(['Refresh']);
		expect(el.querySelector('.pm-closed')?.textContent).toBe('Expired');
		unmount();
	});

	it('an expired card the desktop may not answer still offers Refresh', async () => {
		const { el, unmount } = await mount(
			permit({ exp_ms: Date.now() - 1, desktop_ok: false }),
		);
		expect(labels(el)).toEqual(['Refresh']);
		unmount();
	});

	it('an expired card that is not live or closed has no Refresh', async () => {
		const a = await mount(permit({ exp_ms: Date.now() - 1 }), { live: false });
		expect(labels(a.el)).toEqual([]);
		a.unmount();
		const b = await mount(permit({ exp_ms: Date.now() - 1 }), {
			closed: 'gone',
		});
		expect(labels(b.el)).toEqual([]);
		b.unmount();
	});

	it('Refresh invokes manager_permit_refresh with the card id only and shows Refreshing', async () => {
		const { el, unmount } = await mount(permit({ exp_ms: Date.now() - 1 }));
		await click(el, '.pm-refresh');
		expect(mockInvoke).toHaveBeenCalledWith('manager_permit_refresh', {
			cardId: CARD,
		});
		expect(labels(el)).toEqual([]);
		expect(el.textContent).toContain('Refreshing');
		unmount();
	});

	it('Refresh comes back after a few seconds without an update', async () => {
		jest.useFakeTimers();
		const { el, unmount } = await mount(permit({ exp_ms: Date.now() - 1 }));
		await click(el, '.pm-refresh');
		jest.advanceTimersByTime(10_000);
		await flush();
		expect(labels(el)).toEqual(['Refresh']);
		expect(el.textContent).not.toContain('Refreshing');
		unmount();
	});

	it('a failed refresh shows the error and the button again', async () => {
		mockInvoke.mockRejectedValueOnce('boom');
		const { el, unmount } = await mount(permit({ exp_ms: Date.now() - 1 }));
		await click(el, '.pm-refresh');
		expect(labels(el)).toEqual(['Refresh']);
		expect(el.querySelector('.dc-err')?.textContent).toContain('boom');
		unmount();
	});

	it('the same bubble updates in place when the refreshed card arrives', async () => {
		const state = reactive({
			card: permit({ exp_ms: Date.now() - 1, nonce: 'n1' }),
		});
		const el = document.createElement('div');
		document.body.appendChild(el);
		const app = createApp({
			render: () =>
				h(load('PermitCard'), {
					cardId: CARD,
					permit: state.card,
					live: true,
				}),
		}).use(i18n);
		app.mount(el);
		await flush();
		await click(el, '.pm-refresh');
		expect(el.textContent).toContain('Refreshing');
		state.card = permit({ exp_ms: Date.now() + 60_000, nonce: 'n2' });
		await flush();
		expect(labels(el)).toEqual(['Allow once', 'Always allow', 'Deny']);
		expect(el.querySelector('.pm-closed')).toBeNull();
		expect(el.textContent).not.toContain('Refreshing');
		app.unmount();
	});

	it('a sent card turns expired when exp_ms passes without a close', async () => {
		jest.useFakeTimers();
		const { el, unmount } = await mount(permit({ exp_ms: Date.now() + 5000 }));
		await click(el, '.pm-once');
		expect(el.textContent).toContain('Waiting for the runner');
		jest.advanceTimersByTime(6000);
		await flush();
		expect(el.querySelector('.pm-closed')?.textContent).toBe('Expired');
		expect(el.textContent).not.toContain('Waiting for the runner');
		unmount();
	});

	it('shows the session id in the session label', async () => {
		const { el, unmount } = await mount(permit());
		expect(el.textContent).toContain('Session s_abcdefghij');
		unmount();
	});

	it('strips bidi and format controls from the shown text', async () => {
		const evil = '\u202Ers -rf /\u202C \u200Ex\u2066y\u2069\u061Cz\u200F';
		const { el, unmount } = await mount(
			permit({
				text: evil,
				path: evil,
				reason: evil,
				warning: evil,
				danger: [evil],
			}),
		);
		const shown = el.textContent ?? '';
		expect(shown).not.toMatch(/[\u200E\u200F\u202A-\u202E\u2066-\u2069\u061C]/);
		expect(shown).toContain('rs -rf /');
		unmount();
	});

	const ME = 'a'.repeat(64);
	const touchCard = () =>
		permit({
			desktop_ok: false,
			danger: ['destructive'],
			presence_fps: [ME],
		});

	it('a danger card listing this desktop is answerable with Touch ID', async () => {
		const { el, unmount } = await mount(touchCard(), { selfFp: ME });
		expect(labels(el)).toEqual(['Allow once', 'Always allow', 'Deny']);
		expect(el.textContent).toContain('Touch ID confirms this answer');
		await click(el, '.pm-once');
		await click(el, '.pm-confirm');
		expect(mockInvoke).toHaveBeenCalledWith('manager_permit_answer', {
			cardId: CARD,
			choice: 'once',
			confirmed: true,
		});
		expect(el.textContent).not.toContain('Touch ID confirms this answer');
		unmount();
	});

	it('a danger card listing another desktop stays phone only', async () => {
		const { el, unmount } = await mount(touchCard(), {
			selfFp: 'b'.repeat(64),
		});
		expect(labels(el)).toEqual([]);
		expect(el.textContent).toContain('Answer on your phone or in tmgr attach');
		unmount();
	});

	it('a plain card does not mention Touch ID', async () => {
		const { el, unmount } = await mount(permit(), { selfFp: ME });
		expect(el.textContent).not.toContain('Touch ID');
		unmount();
	});

	it('points to the Encryption tab when Touch ID approvals are not set up', async () => {
		mockInvoke.mockRejectedValueOnce('Set up desktop approvals first');
		const { el, unmount } = await mount(touchCard(), { selfFp: ME });
		await click(el, '.pm-deny');
		expect(el.textContent).toContain(
			'Set up Touch ID approvals in the Encryption tab to answer here',
		);
		expect(labels(el)).toEqual(['Allow once', 'Always allow', 'Deny']);
		unmount();
	});
});

describe('PresenceApprovals', () => {
	const ME = 'a'.repeat(64);
	const KEY = 'c'.repeat(64);
	const mountPresence = async () => {
		const el = document.createElement('div');
		document.body.appendChild(el);
		const app = createApp({
			render: () => h(load('PresenceApprovals'), { selfFp: ME }),
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
	const status = (over = {}) => ({
		available: true,
		set_up: false,
		needs_setup_again: false,
		...over,
	});

	beforeEach(() => jest.resetAllMocks());

	it('is hidden off macOS and without a Secure Enclave', async () => {
		mockInvoke.mockResolvedValueOnce(null);
		const off = await mountPresence();
		expect(off.el.textContent).toBe('');
		off.unmount();
		mockInvoke.mockResolvedValueOnce(status({ available: false }));
		const none = await mountPresence();
		expect(none.el.textContent).toBe('');
		none.unmount();
	});

	it('sets up, reports the runners reached and shows the approve command', async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			Promise.resolve(
				cmd === 'manager_presence_setup'
					? 2
					: status(
							mockInvoke.mock.calls.some(
								(c) => c[0] === 'manager_presence_setup',
							)
								? { set_up: true, key_fp: KEY }
								: {},
						),
			),
		);
		const { el, unmount } = await mountPresence();
		expect(el.textContent).toContain('Not set up');
		await click(el, '.pres-setup');
		expect(el.textContent).toContain('Key sent to runners: 2');
		expect(el.querySelector('.pres-cmd')?.textContent).toBe(
			`tmgr runner manager presence-keys approve ${ME} ${KEY}`,
		);
		expect(el.querySelector('.pres-resend')).not.toBeNull();
		unmount();
	});

	it('asks to set up again when the stored key stopped working', async () => {
		mockInvoke.mockResolvedValue(status({ needs_setup_again: true }));
		const { el, unmount } = await mountPresence();
		expect(el.textContent).toContain('The Touch ID key stopped working');
		expect(el.querySelector('.pres-setup')).not.toBeNull();
		unmount();
	});

	it('shows an open window and locks it', async () => {
		mockInvoke.mockImplementation((cmd: string) =>
			Promise.resolve(
				cmd === 'manager_presence_lock'
					? undefined
					: status({
							set_up: true,
							key_fp: KEY,
							unlocked_secs: mockInvoke.mock.calls.some(
								(c) => c[0] === 'manager_presence_lock',
							)
								? undefined
								: 61,
						}),
			),
		);
		const { el, unmount } = await mountPresence();
		expect(el.textContent).toContain('Unlocked for 2 min');
		await click(el, '.pres-lock');
		expect(mockInvoke).toHaveBeenCalledWith('manager_presence_lock');
		expect(el.textContent).not.toContain('Unlocked for');
		unmount();
	});
});

const dmsg = (
	id: string,
	poster: string,
	cardId: string,
	over: Partial<NonNullable<ChatMessage['decision']>> = {},
): ChatMessage =>
	({
		id,
		decision: { card_id: cardId, poster, ...over },
	}) as unknown as ChatMessage;

describe('permit card grouping', () => {
	const stamp = (nonce: string, exp: number) => permit({ nonce, exp_ms: exp });

	it('10 updates of one card from one poster render exactly one card at the first position with the newest state', () => {
		const msgs = Array.from({ length: 10 }, (_, i) =>
			dmsg(`m${i}`, 'rA', 'c1', { permit: stamp(`n${i}`, 1000 + i) }),
		);
		const groups = permitGroups(msgs);
		expect(Object.keys(groups)).toEqual(['m0']);
		expect(groups.m0.decision.permit?.nonce).toBe('n9');
		expect(groups.m0.decision.permit?.exp_ms).toBe(1009);
		expect(groups.m0.closed).toBeUndefined();
	});

	it('the same card id from another poster does not merge', () => {
		const msgs = [
			dmsg('m1', 'rA', 'c1', { permit: stamp('a1', 1) }),
			dmsg('m2', 'rB', 'c1', { permit: stamp('b1', 2) }),
			dmsg('m3', 'rA', 'c1', { permit: stamp('a2', 3) }),
		];
		const groups = permitGroups(msgs);
		expect(Object.keys(groups).sort()).toEqual(['m1', 'm2']);
		expect(groups.m1.decision.permit?.nonce).toBe('a2');
		expect(groups.m2.decision.permit?.nonce).toBe('b1');
	});

	it('a close from another poster does not close the card', () => {
		const msgs = [
			dmsg('m1', 'rA', 'c1', { permit: permit() }),
			dmsg('m2', 'rB', 'c1', { closed: 'gone' }),
		];
		expect(permitGroups(msgs).m1.closed).toBeUndefined();
	});

	it('a close from the same poster closes the card', () => {
		const msgs = [
			dmsg('m1', 'rA', 'c1', { permit: permit() }),
			dmsg('m2', 'rA', 'c1', { closed: 'answered on Phone' }),
		];
		expect(permitGroups(msgs).m1.closed).toBe('answered on Phone');
	});

	it('a close only applies to cards before it; a later card with the same id is a new open card', () => {
		const msgs = [
			dmsg('m1', 'rA', 'c1', { permit: stamp('n1', 1) }),
			dmsg('m2', 'rA', 'c1', { closed: 'gone' }),
			dmsg('m3', 'rA', 'c1', { permit: stamp('n2', 2) }),
			dmsg('m4', 'rA', 'c1', { permit: stamp('n3', 3) }),
		];
		const groups = permitGroups(msgs);
		expect(Object.keys(groups).sort()).toEqual(['m1', 'm3']);
		expect(groups.m1.closed).toBe('gone');
		expect(groups.m1.decision.permit?.nonce).toBe('n1');
		expect(groups.m3.closed).toBeUndefined();
		expect(groups.m3.decision.permit?.nonce).toBe('n3');
	});

	it('a close that precedes every card does not affect them', () => {
		const msgs = [
			dmsg('m1', 'rA', 'c1', { closed: 'gone' }),
			dmsg('m2', 'rA', 'c1', { permit: permit() }),
		];
		expect(permitGroups(msgs).m2.closed).toBeUndefined();
	});
});
