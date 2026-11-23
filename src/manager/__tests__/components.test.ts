/**
 * @jest-environment jsdom
 */
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import {
	createApp,
	createSSRApp,
	h,
	nextTick,
	reactive,
	type Component,
} from 'vue';
import { createI18n } from 'vue-i18n';
import { renderToString } from 'vue/server-renderer';
import manager from '../../locales/en/manager.json';
import { renderBody } from '../format';
import { speechSynthesisEngine, type TtsEngine } from '../tts/engine';
import { externalVoice } from '../tts/externalVoice';
import { setEngine, speaker } from '../tts/speaker';
import { voicePack, voicePacks } from '../tts/voicePack';
import {
	roomKey,
	type ActivityView,
	type ChatMessage,
	type RoomInfo,
	type SlashCommand,
} from '../types';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));

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
	if (id === '../scrollLogic') return require('../scrollLogic');
	return require(id);
};

const i18n = createI18n({
	legacy: true,
	locale: 'en',
	messages: { en: { manager } },
});

const render = (name: string, props: Record<string, unknown>, slots = {}) =>
	renderToString(
		createSSRApp({
			render: () => require('vue').h(load(name), props, slots),
		}).use(i18n),
	);

const T0 = new Date(2026, 10, 21, 9, 15).getTime();
const mountDom = (name: string, props: Record<string, unknown>) => {
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

const mgrRoom = { kind: 'manager' as const, machine: 'mac' };
const sessRoom = { kind: 'session' as const, machine: 'mac', session: 's1' };

const msg = (over: Partial<ChatMessage> & { id: string }): ChatMessage => ({
	v: 1,
	room: mgrRoom,
	author: { kind: 'manager', id: 'm' },
	ts_ms: T0,
	kind: 'text',
	body_md: 'hello',
	...over,
});

const rooms: RoomInfo[] = [
	{
		room: sessRoom,
		title: 'claude · TM-1021',
		agent: 'claude',
		state: 'working',
		context_pct: 71,
		context_tokens: 142_000,
		last: msg({ id: 'l1', room: sessRoom, body_md: 'Running **tests**' }),
	},
	{ room: mgrRoom, title: 'Manager', last: msg({ id: 'l2' }) },
];

describe('manager components', () => {
	it('ChatList puts the pinned Manager room first with ctx, status and unread', async () => {
		const html = await render('ChatList', {
			rooms,
			activeKey: roomKey(sessRoom),
			unread: { [roomKey(sessRoom)]: 3 },
		});
		expect(html.indexOf('Manager')).toBeLessThan(
			html.indexOf('claude · TM-1021'),
		);
		expect(html).toContain('Pinned');
		expect(html).toContain('142k');
		expect(html).toContain('Working');
		expect(html).toContain('<span class="ub">3</span>');
		expect(html).toContain('role="listbox"');
		expect(html).toContain('aria-selected="true"');
		expect(html).toContain('Running tests');
		expect(html).toContain('09:15');
	});

	it('ChatList emits the room key when a session row is clicked', () => {
		const onSelect = jest.fn();
		const { el, unmount } = mountDom('ChatList', {
			rooms,
			activeKey: null,
			onSelect,
		});
		(el.querySelectorAll('button.crow')[1] as HTMLElement).click();
		expect(onSelect).toHaveBeenCalledWith(roomKey(sessRoom));
		unmount();
	});

	it('SwitchAgentDialog chip shows the model label and emits the alias', () => {
		const onUpdate = jest.fn();
		const { el, unmount } = mountDom('SwitchAgentDialog', {
			agent: 'claude',
			model: 'opus',
			models: ['opus', 'sonnet', 'haiku', 'fable'],
			current: 'claude',
			currentModel: 'opus',
			'onUpdate:model': onUpdate,
		});
		const chip = Array.from(
			el.querySelectorAll<HTMLButtonElement>('button.mchip'),
		).find((b) => b.textContent?.trim() === 'Sonnet 5.5');
		expect(chip).toBeDefined();
		chip!.click();
		expect(onUpdate).toHaveBeenCalledWith('sonnet');
		unmount();
	});

	it('ChatList emits the room key when the pinned Manager row is clicked', () => {
		const onSelect = jest.fn();
		const { el, unmount } = mountDom('ChatList', {
			rooms,
			activeKey: null,
			onSelect,
		});
		(el.querySelectorAll('button.crow')[0] as HTMLElement).click();
		expect(onSelect).toHaveBeenCalledWith(roomKey(mgrRoom));
		unmount();
	});

	it('ChatList shows the no-sessions hint when only the Manager room exists', async () => {
		const html = await render('ChatList', {
			rooms: [{ room: mgrRoom, title: 'Manager' }],
		});
		expect(html).toContain('class="mgr-hint"');
		expect(html).toContain('No sessions yet.');
	});

	it('ChatList hides the no-sessions hint once a session exists', async () => {
		const html = await render('ChatList', { rooms });
		expect(html).not.toContain('mgr-hint');
	});

	it('ChatList prefixes the preview with You when the owner typed the last runner message', async () => {
		const last = msg({
			id: 'u1',
			room: sessRoom,
			author: { kind: 'session', id: 's1', agent: 'claude' },
			user_turn: true,
			body_md: 'fix the build',
		});
		const html = await render('ChatList', {
			rooms: [{ ...rooms[0], last }],
		});
		expect(html).toContain('You: fix the build');
	});

	it('ChatList shows no You prefix for an agent reply in a session', async () => {
		const last = msg({
			id: 'a1',
			room: sessRoom,
			author: { kind: 'session', id: 's1', agent: 'claude' },
			body_md: 'build is green',
		});
		const html = await render('ChatList', {
			rooms: [{ ...rooms[0], last }],
		});
		expect(html).toContain('build is green');
		expect(html).not.toContain('You: ');
	});

	it('ChatHeader shows title, sub, status and full context', async () => {
		const html = await render('ChatHeader', {
			info: rooms[0],
			sub: 'TM-1021 · Cursor pagination',
		});
		expect(html).toContain('claude · TM-1021');
		expect(html).toContain('TM-1021 · Cursor pagination');
		expect(html).toContain('142k / 200k · 71%');
		expect(html).toContain('aria-label="Search in chat"');
	});

	it('ChatHeader shows the agent and model pill only for sessions with a model', async () => {
		const on = await render('ChatHeader', {
			info: { ...rooms[0], agent: 'claude-code', model: 'claude-opus-4-1' },
		});
		expect(on).toContain('Claude Code · opus');
		const codex = await render('ChatHeader', {
			info: { ...rooms[0], agent: 'codex', model: 'gpt-5-codex' },
		});
		expect(codex).toContain('Codex · gpt-5-codex');
		const off = await render('ChatHeader', {
			info: { ...rooms[0], model: undefined },
		});
		expect(off).not.toContain('mdlp');
		const manager = await render('ChatHeader', {
			info: {
				room: mgrRoom,
				title: 'Manager',
				agent: 'claude-code',
				model: 'claude-opus-4-1',
			},
		});
		expect(manager).not.toContain('mdlp');
	});

	it('StatusBar shows scope, chat and Live only when live', async () => {
		const on = await render('StatusBar', {
			scope: 'Personal',
			chat: 'Manager',
			live: true,
		});
		expect(on).toContain('Personal');
		expect(on).toContain('Manager');
		expect(on).toContain('Live');
		const off = await render('StatusBar', { scope: 'Personal', chat: 'M' });
		expect(off).not.toContain('Live');
		expect(off).not.toContain('Do not disturb');
	});

	it('Composer shows the slash argument hint bar only with a hint', async () => {
		const cmd: SlashCommand = {
			name: 'model',
			description: 'Switch model',
			args: '<name>',
			group: 'g',
			dangerous: false,
		};
		const on = await render('Composer', { modelValue: '/model ', hint: cmd });
		expect(on).toContain('hintbar');
		expect(on).toContain('&lt;name&gt;');
		const off = await render('Composer', { modelValue: '' });
		expect(off).not.toContain('hintbar');
	});

	it('ChatHeader shows the Unrestricted badge only for unrestricted rooms', async () => {
		const on = await render('ChatHeader', {
			info: { ...rooms[0], unrestricted: true },
		});
		expect(on).toContain('Unrestricted');
		expect(on).toContain('type into bypass sessions');
		const off = await render('ChatHeader', { info: rooms[0] });
		expect(off).not.toContain('Unrestricted');
	});

	describe('ChatHeader control buttons', () => {
		const labels = (el: HTMLElement) =>
			Array.from(el.querySelectorAll<HTMLButtonElement>('button.ctl')).map(
				(b) => b.textContent?.trim(),
			);
		const header = (info: RoomInfo, extra: Record<string, unknown> = {}) =>
			mountDom('ChatHeader', { info, ...extra });

		it('running local session shows Stop and Force stop', () => {
			const { el, unmount } = header(rooms[0]);
			expect(labels(el)).toEqual(['Stop', 'Force stop']);
			unmount();
		});

		it('exited local session shows only Delete', () => {
			const { el, unmount } = header({ ...rooms[0], state: 'exited' });
			expect(labels(el)).toEqual(['Delete']);
			unmount();
		});

		it('a session without a state yet can be stopped', () => {
			const { el, unmount } = header({ ...rooms[0], state: undefined });
			expect(labels(el)).toEqual(['Stop', 'Force stop']);
			unmount();
		});

		it('manager room and remote rooms show no controls', () => {
			for (const info of [
				rooms[1],
				{ ...rooms[0], local: false },
				{ ...rooms[0], state: 'exited', local: false },
			]) {
				const { el, unmount } = header(info);
				expect(labels(el)).toEqual([]);
				unmount();
			}
		});

		it('buttons emit and busy disables them', () => {
			const onStop = jest.fn();
			const onForce = jest.fn();
			const onDelete = jest.fn();
			const a = header(rooms[0], { onStop, onForce });
			const [stop, force] = Array.from(
				a.el.querySelectorAll<HTMLButtonElement>('button.ctl'),
			);
			stop.click();
			force.click();
			expect(onStop).toHaveBeenCalledTimes(1);
			expect(onForce).toHaveBeenCalledTimes(1);
			a.unmount();
			const b = header({ ...rooms[0], state: 'exited' }, { onDelete });
			b.el.querySelector<HTMLButtonElement>('button.ctl')!.click();
			expect(onDelete).toHaveBeenCalledTimes(1);
			b.unmount();
			const c = header(rooms[0], { busy: true });
			for (const btn of c.el.querySelectorAll<HTMLButtonElement>('button.ctl'))
				expect(btn.disabled).toBe(true);
			c.unmount();
		});
	});

	describe('ControlDialog', () => {
		it('delete dialog names the chat and says it cannot be undone', async () => {
			const html = await render('ControlDialog', {
				mode: 'delete',
				title: 'claude · TM-1021',
			});
			expect(html).toContain('role="dialog"');
			expect(html).toContain('aria-modal="true"');
			expect(html).toContain('claude · TM-1021');
			expect(html).toContain('cannot be undone');
			expect(html).toContain('runner');
			expect(html).toContain('data-autofocus');
		});

		it('dialog is described by its body and exposes busy state', () => {
			const a = mountDom('ControlDialog', { mode: 'delete', title: 'x' });
			const dlg = a.el.querySelector('[role="dialog"]')!;
			expect(dlg.getAttribute('aria-describedby')).toBe('mgr-control-body');
			expect(a.el.querySelector('#mgr-control-body')).not.toBeNull();
			expect(dlg.getAttribute('aria-busy')).toBe('false');
			a.unmount();
			const b = mountDom('ControlDialog', {
				mode: 'delete',
				title: 'x',
				busy: true,
			});
			expect(
				b.el.querySelector('[role="dialog"]')!.getAttribute('aria-busy'),
			).toBe('true');
			b.unmount();
		});

		it('cancel while busy shows a hint instead of emitting', async () => {
			const onCancel = jest.fn();
			const a = mountDom('ControlDialog', {
				mode: 'force',
				title: 'x',
				busy: true,
				onCancel,
			});
			a.el.querySelector<HTMLButtonElement>('button[data-autofocus]')!.click();
			await nextTick();
			expect(onCancel).not.toHaveBeenCalled();
			expect(a.el.querySelector('[role="status"]')?.textContent).toContain(
				'please wait',
			);
			a.unmount();
		});

		it('focus returns to the opener when the dialog closes', async () => {
			const opener = document.createElement('button');
			document.body.appendChild(opener);
			opener.focus();
			const a = mountDom('ControlDialog', { mode: 'delete', title: 'x' });
			expect(document.activeElement).not.toBe(opener);
			a.unmount();
			expect(document.activeElement).toBe(opener);
			opener.remove();
		});

		it('force dialog warns about killing the process', async () => {
			const html = await render('ControlDialog', {
				mode: 'force',
				title: 'x',
			});
			expect(html).toContain('Force stop');
			expect(html).toContain('immediately');
		});

		it('confirm and cancel emit, busy disables confirm, error is an alert', async () => {
			const onConfirm = jest.fn();
			const onCancel = jest.fn();
			const a = mountDom('ControlDialog', {
				mode: 'delete',
				title: 'x',
				onConfirm,
				onCancel,
			});
			const [cancel, confirm] = Array.from(
				a.el.querySelectorAll<HTMLButtonElement>('.trust-actions button'),
			);
			cancel.click();
			expect(onCancel).toHaveBeenCalledTimes(1);
			expect(onConfirm).not.toHaveBeenCalled();
			confirm.click();
			expect(onConfirm).toHaveBeenCalledTimes(1);
			a.unmount();
			const b = mountDom('ControlDialog', {
				mode: 'delete',
				title: 'x',
				busy: true,
				error: 'nope',
			});
			const buttons = b.el.querySelectorAll<HTMLButtonElement>(
				'.trust-actions button',
			);
			expect(buttons[1].disabled).toBe(true);
			expect(b.el.querySelector('.mgr-error[role="alert"]')?.textContent).toBe(
				'nope',
			);
			b.unmount();
		});
	});

	it('ChatHeader bar and title are window drag regions, buttons are not', async () => {
		const html = await render('ChatHeader', { info: rooms[0], sub: 'x' });
		expect(html).toMatch(/<div class="mh2" data-tauri-drag-region/);
		expect(html).toMatch(/<div class="t" data-tauri-drag-region/);
		expect(html).toMatch(/<b data-tauri-drag-region/);
		expect(html).not.toMatch(/<button[^>]*data-tauri-drag-region/);
	});

	it('MessageBubble labels a queued owner message and offers Cancel', async () => {
		const html = await render('MessageBubble', {
			message: msg({
				id: 'q1',
				author: { kind: 'owner', id: 'me' },
				body_md: 'Run the tests',
			}),
			queued: true,
		});
		expect(html).toContain('Queued');
		expect(html).toContain('Cancel');
		expect(html).toContain('Waiting for the session to be at its prompt');
	});

	it('MessageBubble drops the queue label once the message has left the queue', async () => {
		const html = await render('MessageBubble', {
			message: msg({
				id: 'q1',
				author: { kind: 'owner', id: 'me' },
				body_md: 'Run the tests',
			}),
			queued: false,
		});
		expect(html).not.toContain('Queued');
		expect(html).not.toContain('>Cancel<');
	});

	it('MessageBubble emits cancel-queued with the message id when Cancel is clicked', () => {
		const onCancelQueued = jest.fn();
		const { el, unmount } = mountDom('MessageBubble', {
			message: msg({
				id: 'q1',
				author: { kind: 'owner', id: 'me' },
			}),
			queued: true,
			onCancelQueued,
		});
		(el.querySelector('button.qcancel') as HTMLElement).click();
		expect(onCancelQueued).toHaveBeenCalledWith('q1');
		unmount();
	});

	describe('MessageBubble delivery status', () => {
		const own = msg({ id: 'd1', author: { kind: 'owner', id: 'me' } });
		const states: Array<[Record<string, unknown>, string]> = [
			[{ state: 'sending', remote: true }, 'Sending…'],
			[{ state: 'sent', remote: true }, 'Sent'],
			[
				{ state: 'queued', remote: true },
				'Queued on claude on mac, waiting for the agent',
			],
			[{ state: 'delivered', remote: true }, 'Delivered'],
			[
				{ state: 'failed', remote: true, reason: 'busy' },
				'Not delivered: busy',
			],
			[{ state: 'unconfirmed', remote: true }, 'Sent, not confirmed'],
		];

		it.each(states)('renders %j as "%s"', async (delivery, text) => {
			const html = await render('MessageBubble', {
				message: own,
				delivery,
				deliveryWho: 'claude on mac',
			});
			expect(html).toContain(text);
			expect(html).toContain('role="status"');
		});

		it('shows no status line without a delivery', async () => {
			const html = await render('MessageBubble', { message: own });
			expect(html).not.toContain('dlv');
		});

		it('offers Retry for failed and emits retry with the message id', () => {
			const onRetry = jest.fn();
			const { el, unmount } = mountDom('MessageBubble', {
				message: own,
				delivery: { state: 'failed', remote: true, reason: 'x' },
				onRetry,
			});
			const btn = el.querySelector('button.qcancel') as HTMLElement;
			expect(btn.textContent).toContain('Retry');
			btn.click();
			expect(onRetry).toHaveBeenCalledWith('d1');
			unmount();
		});

		it('labels the not-confirmed action Resend with a title about the first copy', async () => {
			const html = await render('MessageBubble', {
				message: own,
				delivery: { state: 'unconfirmed', remote: true },
			});
			expect(html).toContain('Resend');
			expect(html).toContain('The first copy may still arrive');
		});

		it('hides Retry for sent, delivered and already retried rows', async () => {
			for (const delivery of [
				{ state: 'sent', remote: true },
				{ state: 'delivered', remote: true },
				{ state: 'failed', remote: true, retried: true },
			]) {
				const html = await render('MessageBubble', { message: own, delivery });
				expect(html).not.toContain('Retry');
				expect(html).not.toContain('Resend');
			}
		});
	});

	it('MessageList shows the default empty hint without a custom one', async () => {
		const html = await render('MessageList', { messages: [] });
		expect(html).toContain('No messages in this chat.');
	});

	it('MessageList shows a custom empty hint instead of the default', async () => {
		const html = await render('MessageList', {
			messages: [],
			emptyHint: 'Replies appear from now on.',
		});
		expect(html).toContain('Replies appear from now on.');
		expect(html).not.toContain('No messages in this chat.');
	});

	it('MessageList renders day separator, grouping, reply quote, system and decision slot', async () => {
		const messages = [
			msg({
				id: 'a',
				author: { kind: 'owner', id: 'me' },
				body_md: 'Start it',
			}),
			msg({ id: 'b', ts_ms: T0 + 60_000, body_md: 'On it' }),
			msg({ id: 'c', ts_ms: T0 + 90_000, body_md: 'Done' }),
			msg({
				id: 'd',
				author: { kind: 'owner', id: 'me' },
				ts_ms: T0 + 120_000,
				body_md: 'Thanks',
				reply_to: 'b',
				attachments: [
					{ name: 'log.txt', mime: 'text/plain', size: 2048, sha256: 'x' },
				],
			}),
			msg({
				id: 'e',
				kind: 'system',
				author: { kind: 'system', id: 'sys' },
				body_md: 'Session started',
			}),
			msg({ id: 'f', kind: 'mystery', body_md: 'Unknown kind' }),
			msg({
				id: 'g',
				kind: 'decision',
				decision: { card_id: 'c1' },
				body_md: '',
			}),
		];
		const html = await render(
			'MessageList',
			{ messages, bodyHtml: { b: '<p>On <strong>it</strong></p>' }, now: T0 },
			{
				decision: ({ message }: { message: ChatMessage }) =>
					`CARD:${message.decision?.card_id}`,
			},
		);
		expect(html).toContain('class="day">Today');
		expect(html.match(/class="who"/g)).toHaveLength(2);
		expect(html).toContain('<strong>it</strong>');
		expect(html).toContain('class="qt"');
		expect(html).toContain('log.txt');
		expect(html).toContain('2 KB');
		expect(html).toContain('class="sys"');
		expect(html).toContain('Unknown kind');
		expect(html).toContain('CARD:c1');
		expect(html).toContain('aria-label="Reply"');
		expect(html).toContain('aria-label="Copy"');
	});

	it('Composer shows reply bar, attachment chips with remove, attach and send', async () => {
		const html = await render('Composer', {
			modelValue: 'hi',
			replyTo: msg({ id: 'b', body_md: 'On it' }),
			attachments: [{ name: 'a.png', mime: 'image/png', size: 1, sha256: 's' }],
		});
		expect(html).toContain('Replying to Manager');
		expect(html).toContain('a.png');
		expect(html).toContain('aria-label="Remove a.png"');
		expect(html).toContain('aria-label="Attach a file"');
		expect(html).toContain('aria-label="Send"');
		expect(html).not.toContain('disabled');
	});

	it('Composer disables send when empty', async () => {
		const html = await render('Composer', { modelValue: '' });
		expect(html).toContain('disabled');
	});

	it('SlashMenu groups commands, marks the active row and dangerous ones', async () => {
		const commands: SlashCommand[] = [
			{
				name: 'compact',
				description: 'Compact',
				args: '[instructions]',
				group: 'Agent built-ins',
				dangerous: false,
			},
			{
				name: 'clear',
				description: 'Clear history',
				group: 'Agent built-ins',
				dangerous: true,
			},
			{
				name: 'summary',
				description: 'Morning summary',
				group: 'TMGR',
				dangerous: false,
			},
		];
		const html = await render('SlashMenu', { commands, activeIndex: 1 });
		expect(html).toContain('role="listbox"');
		expect(html.match(/role="option"/g)).toHaveLength(3);
		expect(html).toMatch(
			/id="slash-opt-1"[^>]*aria-selected="true"[^>]*class="on"/,
		);
		expect(html).toContain('aria-selected="false"');
		expect(html).toContain('Agent built-ins');
		expect(html).toContain('[instructions]');
		expect(html).toContain('asks you first');
	});

	it('SearchPanel highlights the query and shows the scope toggle', async () => {
		const hit = msg({
			id: 'h',
			room: sessRoom,
			author: { kind: 'session', id: 's1', agent: 'claude' },
			body_md: 'All tests pass',
		});
		const html = await render('SearchPanel', {
			query: 'tests',
			scope: 'chat',
			chatLabel: 'claude · TM-1021',
			hits: [hit],
			roomTitles: { [roomKey(sessRoom)]: 'claude · TM-1021' },
		});
		expect(html).toContain('<mark>tests</mark>');
		expect(html).toContain('1 result');
		expect(html).toContain('In claude · TM-1021');
		expect(html).toMatch(
			/class="sf2 on"[^>]*aria-pressed="true"|aria-pressed="true"[^>]*class="sf2 on"/,
		);
		expect(html).toContain('All chats');
	});

	it('SearchPanel keeps pending filters focusable with aria-disabled and names the group', async () => {
		const html = await render('SearchPanel', {
			query: '',
			pending: ['scopeMe'],
		});
		expect(html).toMatch(/class="sf2 off"[^>]*aria-disabled="true"/);
		expect(html).not.toMatch(/class="sf2 off"[^>]*\sdisabled/);
		expect(html).toContain('Not available yet');
		expect(html).toContain('aria-label="Search filters"');
	});

	it('SearchPanel shows empty states', async () => {
		expect(await render('SearchPanel', { query: '' })).toContain(
			'Search all your chats',
		);
		expect(await render('SearchPanel', { query: 'zzz' })).toContain(
			'No matches',
		);
	});
});

describe('MessageBubble copy buttons', () => {
	const flushMicrotasks = async () => {
		for (let i = 0; i < 5; i++) await Promise.resolve();
	};

	const originalClipboard = Object.getOwnPropertyDescriptor(
		navigator,
		'clipboard',
	);

	beforeEach(() => jest.useFakeTimers());
	afterEach(() => {
		jest.useRealTimers();
		if (originalClipboard) {
			Object.defineProperty(navigator, 'clipboard', originalClipboard);
		} else {
			delete (navigator as { clipboard?: unknown }).clipboard;
		}
	});

	it('copies the code of an agent message and shows Copied for 1.5 s', async () => {
		const writeText = jest.fn().mockResolvedValue(undefined);
		Object.defineProperty(navigator, 'clipboard', {
			value: { writeText },
			configurable: true,
		});
		const m = msg({ id: 'c1', body_md: '```\nconst a = 1;\n```' });
		const mounted = mountDom('MessageBubble', {
			message: m,
			html: renderBody(m.body_md),
		});
		const btn = mounted.el.querySelector<HTMLButtonElement>('button.blk-copy');
		expect(btn?.textContent).toBe('Copy');

		btn!.click();
		await flushMicrotasks();
		expect(writeText).toHaveBeenCalledWith('const a = 1;');
		expect(btn!.textContent).toBe('Copied');

		jest.advanceTimersByTime(1500);
		expect(btn!.textContent).toBe('Copy');
		mounted.unmount();
	});

	it('adds no copy buttons to owner messages', () => {
		const m = msg({
			id: 'o1',
			author: { kind: 'owner', id: 'me' },
			body_md: '```\nx\n```',
		});
		const mounted = mountDom('MessageBubble', {
			message: m,
			html: renderBody(m.body_md),
		});
		expect(mounted.el.querySelector('.blk-copy')).toBeNull();
		mounted.unmount();
	});
});

describe('MessageList scroll behaviour', () => {
	const mounted: Array<() => void> = [];
	const ROW = 100;
	const VIEW = 400;
	const settle = async () => {
		for (let i = 0; i < 3; i++) await nextTick();
	};
	const deferred = () => {
		let resolve: (grew: boolean | null) => void = () => undefined;
		const promise = new Promise<boolean | null>((res) => (resolve = res));
		return { promise, resolve };
	};

	const mountList = (
		initial: ChatMessage[] = long,
		extra: Record<string, unknown> = {},
	) => {
		const state = reactive({
			messages: initial,
			roomKey: 'r1',
			activity: null as ActivityView | null,
		});
		const shownRooms = new Set<string>();
		const el = document.createElement('div');
		document.body.appendChild(el);
		const app = createApp({
			render: () =>
				h(load('MessageList'), {
					messages: state.messages,
					roomKey: state.roomKey,
					activity: state.activity,
					shownRooms,
					...extra,
				}),
		}).use(i18n);
		app.mount(el);
		const scroller = el.querySelector('.scroll') as HTMLElement;
		const rendered = () =>
			(scroller.querySelector('.inner') as HTMLElement).children.length;
		const metrics = {
			extra: 0,
			stored: 0,
			height: () => rendered() * ROW + metrics.extra,
			bottom: () => Math.max(0, metrics.height() - VIEW),
		};
		Object.defineProperties(scroller, {
			scrollHeight: { configurable: true, get: () => metrics.height() },
			clientHeight: { configurable: true, get: () => VIEW },
			scrollTop: {
				configurable: true,
				get: () =>
					(metrics.stored = Math.min(metrics.stored, metrics.bottom())),
				set: (v: number) => {
					metrics.stored = Math.max(0, Math.min(v, metrics.bottom()));
				},
			},
		});
		const scrollTo = (top: number) => {
			scroller.scrollTop = top;
			scroller.dispatchEvent(new Event('scroll'));
		};
		const jump = () =>
			el.querySelector('button.jump') as HTMLButtonElement | null;
		mounted.push(() => {
			app.unmount();
			el.remove();
		});
		return { el, state, scroller, metrics, scrollTo, jump };
	};

	afterEach(() => {
		mounted.splice(0).forEach((unmount) => unmount());
	});

	const row = (i: number) =>
		msg({ id: `m${i}`, ts_ms: T0 + i * 60_000, body_md: `row ${i}` });
	const long = Array.from({ length: 10 }, (_, i) => row(i));
	const older = (id: string) => msg({ id, ts_ms: T0 - 60_000, body_md: id });
	const fromOwner = (id: string, ts = T0 + 90_000) =>
		msg({ id, author: { kind: 'owner', id: 'me' }, ts_ms: ts });
	const fromOther = (id: string, ts = T0 + 90_000) => msg({ id, ts_ms: ts });
	const fromOwnerTurn = (id: string, ts = T0 + 90_000) =>
		msg({
			id,
			author: { kind: 'session', id: 's1', agent: 'claude' },
			user_turn: true,
			ts_ms: ts,
		});

	it('pins to the last message the first time a chat is shown', async () => {
		const { scroller, metrics } = mountList();
		await settle();
		expect(scroller.scrollTop).toBe(metrics.bottom());
	});

	it('shows no jump button while pinned to the bottom', async () => {
		const { jump } = mountList();
		await settle();
		expect(jump()).toBeNull();
	});

	it('keeps the position and counts arrivals when scrolled up', async () => {
		const { state, scroller, scrollTo, jump } = mountList();
		await settle();
		scrollTo(100);
		state.messages = [...state.messages, fromOther('c')];
		await settle();
		expect(scroller.scrollTop).toBe(100);
		expect(jump()?.getAttribute('aria-label')).toBe('1 new message');
		expect(jump()?.textContent).toContain('↓');
		expect(jump()?.textContent).toContain('1');
	});

	it('does not count own messages and scrolls to them', async () => {
		const { state, scroller, metrics, scrollTo, jump } = mountList();
		await settle();
		scrollTo(100);
		state.messages = [...state.messages, fromOwner('d')];
		await settle();
		expect(scroller.scrollTop).toBe(metrics.bottom());
		expect(jump()).toBeNull();
	});

	it('does not count a runner message typed by the owner as an arrival', async () => {
		const { state, scroller, metrics, scrollTo, jump } = mountList();
		await settle();
		scrollTo(100);
		state.messages = [...state.messages, fromOwnerTurn('e')];
		await settle();
		expect(scroller.scrollTop).toBe(metrics.bottom());
		expect(jump()).toBeNull();
	});

	it('smooth-scrolls to the bottom and resets the count on click', async () => {
		const { state, scroller, metrics, scrollTo, jump } = mountList();
		await settle();
		scrollTo(100);
		state.messages = [...state.messages, fromOther('c')];
		await settle();
		const scrollToSpy = jest.fn();
		(scroller as HTMLElement & { scrollTo: unknown }).scrollTo = scrollToSpy;
		jump()!.click();
		await settle();
		expect(scrollToSpy).toHaveBeenCalledWith({
			top: metrics.height(),
			behavior: 'smooth',
		});
		expect(jump()).toBeNull();
	});

	it('moves focus to the message list after jumping', async () => {
		const { state, scroller, scrollTo, jump } = mountList();
		await settle();
		scrollTo(100);
		state.messages = [...state.messages, fromOther('c')];
		await settle();
		(scroller as HTMLElement & { scrollTo: unknown }).scrollTo = jest.fn();
		jump()!.click();
		await settle();
		expect(document.activeElement).toBe(scroller);
	});

	it('ignores mid-animation scroll positions while jumping to the bottom', async () => {
		const { state, scroller, metrics, scrollTo, jump } = mountList();
		await settle();
		scrollTo(100);
		state.messages = [...state.messages, fromOther('c')];
		await settle();
		(scroller as HTMLElement & { scrollTo: unknown }).scrollTo = jest.fn();
		jump()!.click();
		await settle();
		scrollTo(300);
		state.messages = [...state.messages, fromOther('d', T0 + 120_000)];
		await settle();
		expect(jump()).toBeNull();
		expect(scroller.scrollTop).toBe(metrics.bottom());
	});

	it('stops ignoring scroll positions after the jump timeout', async () => {
		jest.useFakeTimers();
		try {
			const { state, scroller, scrollTo, jump } = mountList();
			await settle();
			scrollTo(100);
			state.messages = [...state.messages, fromOther('c')];
			await settle();
			(scroller as HTMLElement & { scrollTo: unknown }).scrollTo = jest.fn();
			jump()!.click();
			await settle();
			jest.advanceTimersByTime(600);
			scrollTo(300);
			state.messages = [...state.messages, fromOther('d', T0 + 120_000)];
			await settle();
			expect(jump()?.getAttribute('aria-label')).toBe('1 new message');
		} finally {
			jest.useRealTimers();
		}
	});

	it('requests older history once while busy and keeps the position', async () => {
		const page = deferred();
		const loadOlder = jest.fn(() => page.promise);
		const { state, scroller, scrollTo } = mountList(long, { loadOlder });
		await settle();
		scrollTo(0);
		scrollTo(0);
		expect(loadOlder).toHaveBeenCalledTimes(1);
		state.messages = [older('z'), ...state.messages];
		page.resolve(true);
		await settle();
		expect(scroller.scrollTop).toBe(100);
	});

	it('keeps the anchor when a row arrives at the bottom before the older page', async () => {
		const page = deferred();
		const loadOlder = jest.fn(() => page.promise);
		const { state, scroller, scrollTo } = mountList(long, { loadOlder });
		await settle();
		scrollTo(0);
		state.messages = [...state.messages, fromOther('c')];
		await settle();
		state.messages = [older('z'), ...state.messages];
		await settle();
		page.resolve(true);
		await settle();
		expect(scroller.scrollTop).toBe(100);
	});

	it('re-pins to the bottom when older rows are prepended while pinned', async () => {
		const { state, scroller, metrics } = mountList();
		await settle();
		state.messages = [older('z'), ...state.messages];
		await settle();
		expect(scroller.scrollTop).toBe(metrics.bottom());
	});

	it('keeps the bottom when rows grow without a message change while pinned', async () => {
		const callbacks: Array<() => void> = [];
		const holder = globalThis as { ResizeObserver?: unknown };
		const original = holder.ResizeObserver;
		holder.ResizeObserver = class {
			constructor(callback: () => void) {
				callbacks.push(callback);
			}
			observe() {}
			unobserve() {}
			disconnect() {}
		};
		try {
			const { metrics, scroller } = mountList();
			await settle();
			metrics.extra = 50;
			callbacks.forEach((callback) => callback());
			await settle();
			expect(scroller.scrollTop).toBe(metrics.bottom());
		} finally {
			holder.ResizeObserver = original;
		}
	});

	it('stops requesting history after a page adds nothing', async () => {
		const loadOlder = jest.fn(async () => false);
		const { scrollTo } = mountList(long, { loadOlder });
		await settle();
		scrollTo(0);
		await settle();
		scrollTo(0);
		await settle();
		expect(loadOlder).toHaveBeenCalledTimes(1);
	});

	it('keeps history retryable after a null (failed) page result', async () => {
		const loadOlder = jest
			.fn()
			.mockResolvedValueOnce(null)
			.mockResolvedValueOnce(false);
		const { scrollTo } = mountList(long, { loadOlder });
		await settle();
		scrollTo(0);
		await settle();
		scrollTo(0);
		await settle();
		scrollTo(0);
		await settle();
		expect(loadOlder).toHaveBeenCalledTimes(2);
	});

	it('keeps history retryable after a rejected page request', async () => {
		const loadOlder = jest
			.fn()
			.mockRejectedValueOnce(new Error('offline'))
			.mockResolvedValueOnce(false);
		const { scrollTo } = mountList(long, { loadOlder });
		await settle();
		scrollTo(0);
		await settle();
		scrollTo(0);
		await settle();
		expect(loadOlder).toHaveBeenCalledTimes(2);
	});

	it('counts an arrival from someone else during loadOlder', async () => {
		const page = deferred();
		const { state, scrollTo, jump } = mountList(long, {
			loadOlder: jest.fn(() => page.promise),
		});
		await settle();
		scrollTo(0);
		state.messages = [...state.messages, fromOther('c')];
		await settle();
		page.resolve(false);
		await settle();
		expect(jump()?.getAttribute('aria-label')).toBe('1 new message');
	});

	it('pins to the bottom for an own message that arrives during loadOlder', async () => {
		const page = deferred();
		const { state, scroller, metrics, scrollTo } = mountList(long, {
			loadOlder: jest.fn(() => page.promise),
		});
		await settle();
		scrollTo(0);
		state.messages = [...state.messages, fromOwner('d')];
		await settle();
		expect(scroller.scrollTop).toBe(metrics.bottom());
		page.resolve(false);
		await settle();
		expect(scroller.scrollTop).toBe(metrics.bottom());
	});

	it('ignores a stale history result after leaving and returning to the room', async () => {
		const first = deferred();
		const second = deferred();
		const loadOlder = jest
			.fn()
			.mockReturnValueOnce(first.promise)
			.mockReturnValueOnce(second.promise);
		const { state, scroller, scrollTo } = mountList(long, { loadOlder });
		await settle();
		scrollTo(0);
		state.roomKey = 'r2';
		await settle();
		state.roomKey = 'r1';
		await settle();
		scrollTo(0);
		expect(loadOlder).toHaveBeenCalledTimes(2);
		first.resolve(false);
		await settle();
		scrollTo(0);
		expect(loadOlder).toHaveBeenCalledTimes(2);
		second.resolve(true);
		await settle();
		scrollTo(0);
		expect(loadOlder).toHaveBeenCalledTimes(3);
		expect(scroller).toBeDefined();
	});

	it('scrolls to the bottom on first visit even when the room was empty before', async () => {
		const { state, scroller, metrics } = mountList([]);
		await settle();
		state.roomKey = 'r2';
		await settle();
		state.roomKey = 'r1';
		await settle();
		state.messages = long;
		await settle();
		expect(scroller.scrollTop).toBe(metrics.bottom());
	});

	it('restores the scroll position of a revisited room', async () => {
		const { state, scroller, scrollTo } = mountList();
		await settle();
		scrollTo(200);
		state.roomKey = 'r2';
		state.messages = [row(20)];
		await settle();
		expect(scroller.scrollTop).toBe(0);
		state.roomKey = 'r1';
		state.messages = long;
		await settle();
		expect(scroller.scrollTop).toBe(200);
	});

	it('pins a revisited room that was left at the bottom', async () => {
		const { state, scroller, metrics } = mountList();
		await settle();
		state.roomKey = 'r2';
		state.messages = [row(20)];
		await settle();
		state.roomKey = 'r1';
		state.messages = long;
		await settle();
		expect(scroller.scrollTop).toBe(metrics.bottom());
	});
});

describe('MessageBubble speak button', () => {
	const flush = () => nextTick();
	const fakeEngine = (available = true) => {
		const calls: Array<{
			hooks: { onEnd(): void; onError(reason: string): void };
		}> = [];
		const engine: TtsEngine = {
			id: 'fake',
			isAvailable: () => available,
			speak: (_chunks, _lang, hooks) => {
				calls.push({ hooks });
			},
			stop: jest.fn(),
		};
		return { engine, calls };
	};
	const speakBtn = (el: HTMLElement) =>
		el.querySelector<HTMLButtonElement>('.macts button[aria-pressed]');

	afterEach(() => setEngine(speechSynthesisEngine));

	it('shows a toggle for an agent reply with text and reads it aloud', async () => {
		const fake = fakeEngine();
		setEngine(fake.engine);
		const mounted = mountDom('MessageBubble', {
			message: msg({ id: 's1', body_md: 'Hello there.' }),
			html: renderBody('Hello there.'),
		});
		const btn = speakBtn(mounted.el);
		expect(btn?.getAttribute('aria-pressed')).toBe('false');
		expect(btn?.getAttribute('aria-label')).toBe('Speak');

		btn!.click();
		await flush();
		expect(speakBtn(mounted.el)?.getAttribute('aria-pressed')).toBe('true');
		expect(speakBtn(mounted.el)?.getAttribute('aria-label')).toBe(
			'Stop speaking',
		);

		speakBtn(mounted.el)!.click();
		await flush();
		expect(speakBtn(mounted.el)?.getAttribute('aria-pressed')).toBe('false');
		mounted.unmount();
	});

	it('shows the no-voice message as the title', async () => {
		const fake = fakeEngine();
		setEngine(fake.engine);
		const mounted = mountDom('MessageBubble', {
			message: msg({ id: 's2', body_md: 'Привет.' }),
			html: renderBody('Привет.'),
		});
		speakBtn(mounted.el)!.click();
		fake.calls[0].hooks.onError('no-voice');
		await flush();
		expect(speakBtn(mounted.el)?.getAttribute('title')).toBe(
			'No voice for this language',
		);
		mounted.unmount();
	});

	it('is hidden for owner messages, decisions, empty text and an unavailable engine', () => {
		setEngine(fakeEngine().engine);
		const owner = mountDom('MessageBubble', {
			message: msg({
				id: 'o2',
				author: { kind: 'owner', id: 'me' },
				body_md: 'mine',
			}),
			html: renderBody('mine'),
		});
		expect(speakBtn(owner.el)).toBeNull();
		owner.unmount();

		const decision = mountDom('MessageBubble', {
			message: msg({ id: 'd1', kind: 'decision', body_md: 'Pick one' }),
		});
		expect(speakBtn(decision.el)).toBeNull();
		decision.unmount();

		const empty = mountDom('MessageBubble', {
			message: msg({ id: 'e1', body_md: '  ' }),
		});
		expect(speakBtn(empty.el)).toBeNull();
		empty.unmount();

		setEngine(fakeEngine(false).engine);
		const unavailable = mountDom('MessageBubble', {
			message: msg({ id: 'u1', body_md: 'Hello.' }),
			html: renderBody('Hello.'),
		});
		expect(speakBtn(unavailable.el)).toBeNull();
		unavailable.unmount();
	});
});

describe('live tool activity UI', () => {
	it('MessageBubble lists the used tools as plain text in a collapsed details element', () => {
		const { el, unmount } = mountDom('MessageBubble', {
			message: msg({ id: 'a1' }),
			tools: ['Read: x.ts', '<img src=x onerror=alert(1)>'],
		});
		const details = el.querySelector('details.tools') as HTMLDetailsElement;
		expect(details.open).toBe(false);
		expect(details.querySelector('summary')?.textContent).toBe(
			'Tools used (2)',
		);
		const items = [...details.querySelectorAll('li')].map(
			(li) => li.textContent,
		);
		expect(items).toEqual(['Read: x.ts', '<img src=x onerror=alert(1)>']);
		expect(details.querySelector('img')).toBeNull();
		unmount();
	});

	it('MessageBubble shows call rows with a monospace summary and expands the result on click', () => {
		const { el, unmount } = mountDom('MessageBubble', {
			message: msg({ id: 'a1' }),
			tools: ['Bash: ls', 'Read: x.ts'],
			calls: [
				{
					n: 1,
					name: 'Bash',
					summary: 'ls -la <img src=x onerror=alert(1)>',
					result: 'file1\nfile2',
				},
				{ n: 2, name: 'Read', summary: '/a/x.ts' },
			],
		});
		const details = el.querySelector('details.tools') as HTMLDetailsElement;
		const rows = [...details.querySelectorAll('li.call')];
		expect(rows).toHaveLength(2);
		expect(rows[0].querySelector('.call-name')?.textContent).toBe('Bash');
		expect(rows[0].querySelector('code.call-sum')?.textContent).toBe(
			'ls -la <img src=x onerror=alert(1)>',
		);
		expect(details.querySelector('img')).toBeNull();
		expect(rows[0].querySelector('pre')).toBeNull();
		(rows[0].querySelector('button') as HTMLButtonElement).click();
		return Promise.resolve()
			.then(() => Promise.resolve())
			.then(() => {
				expect(rows[0].querySelector('pre.call-out')?.textContent).toBe(
					'file1\nfile2',
				);
				expect(rows[0].classList.contains('open')).toBe(true);
				unmount();
			});
	});

	it('MessageBubble shows no tools list without tools or for owner messages', () => {
		const none = mountDom('MessageBubble', { message: msg({ id: 'a2' }) });
		expect(none.el.querySelector('details.tools')).toBeNull();
		none.unmount();
		const owner = mountDom('MessageBubble', {
			message: msg({ id: 'a3', author: { kind: 'owner', id: 'me' } }),
			tools: ['Read: x.ts'],
		});
		expect(owner.el.querySelector('details.tools')).toBeNull();
		owner.unmount();
	});

	it('MessageList shows the live line only while working, with the current label or a fallback', async () => {
		const html = (activity: ActivityView | null) =>
			render('MessageList', { messages: [msg({ id: 'a1' })], activity });
		const live = await html({
			state: 'working',
			current: 'Bash: cargo test',
			used: ['Bash: cargo test'],
		});
		expect(live).toContain('class="live"');
		expect(live).toContain('aria-live="polite"');
		expect(live).toContain('Bash: cargo test');
		const fallback = await html({ state: 'working', current: null, used: [] });
		expect(fallback).toContain('Working…');
		expect(await html(null)).not.toContain('class="live"');
	});

	describe('pinned scrolling', () => {
		const ROW = 100;
		const VIEW = 400;
		const settle = async () => {
			for (let i = 0; i < 3; i++) await nextTick();
		};
		const working: ActivityView = {
			state: 'working',
			current: 'Read: x.ts',
			used: ['Read: x.ts'],
		};

		const mountScrolled = () => {
			const state = reactive({
				activity: null as ActivityView | null,
			});
			const messages = Array.from({ length: 10 }, (_, i) =>
				msg({ id: `m${i}`, ts_ms: T0 + i * 60_000 }),
			);
			const el = document.createElement('div');
			document.body.appendChild(el);
			const app = createApp({
				render: () =>
					h(load('MessageList'), {
						messages,
						roomKey: 'r1',
						shownRooms: new Set<string>(),
						activity: state.activity,
					}),
			}).use(i18n);
			app.mount(el);
			const scroller = el.querySelector('.scroll') as HTMLElement;
			let stored = 0;
			const height = () =>
				(scroller.querySelector('.inner') as HTMLElement).children.length * ROW;
			Object.defineProperties(scroller, {
				scrollHeight: { configurable: true, get: height },
				clientHeight: { configurable: true, get: () => VIEW },
				scrollTop: {
					configurable: true,
					get: () => (stored = Math.min(stored, Math.max(0, height() - VIEW))),
					set: (v: number) => {
						stored = Math.max(0, Math.min(v, Math.max(0, height() - VIEW)));
					},
				},
			});
			return { el, app, state, scroller, height };
		};

		it('a live line appearing and disappearing does not move or unpin a scrolled-up list', async () => {
			const { el, app, state, scroller } = mountScrolled();
			await settle();
			scroller.scrollTop = 100;
			scroller.dispatchEvent(new Event('scroll'));
			state.activity = working;
			await settle();
			expect(el.querySelector('.live')).not.toBeNull();
			expect(scroller.scrollTop).toBe(100);
			expect(el.querySelector('button.jump')).toBeNull();
			state.activity = null;
			await settle();
			expect(el.querySelector('.live')).toBeNull();
			expect(scroller.scrollTop).toBe(100);
			app.unmount();
			el.remove();
		});

		it('stays pinned to the bottom across a live line', async () => {
			const { el, app, state, scroller, height } = mountScrolled();
			await settle();
			const bottom = () => height() - VIEW;
			expect(scroller.scrollTop).toBe(bottom());
			state.activity = working;
			await settle();
			state.activity = null;
			await settle();
			expect(el.querySelector('button.jump')).toBeNull();
			expect(scroller.scrollTop).toBe(bottom());
			app.unmount();
			el.remove();
		});
	});
});

describe('MessageBubble English voice control', () => {
	const MB = 94 * 1024 * 1024;
	const voiceBox = (el: HTMLElement) => el.querySelector<HTMLElement>('.voice');
	const voiceButtons = (el: HTMLElement) =>
		Array.from(el.querySelectorAll<HTMLButtonElement>('.voice button'));
	const buttonByText = (el: HTMLElement, text: string) =>
		voiceButtons(el).find((b) => b.textContent?.trim() === text);
	let refresh: jest.SpyInstance;

	beforeEach(() => {
		voicePack.supported = true;
		voicePack.installed = false;
		voicePack.installing = false;
		voicePack.progress = 0;
		voicePack.error = null;
		voicePack.sizeBytes = MB;
		voicePack.statusRequested = false;
		refresh = jest.spyOn(voicePack, 'refresh').mockResolvedValue();
	});

	afterEach(() => {
		refresh.mockRestore();
		voicePack.supported = false;
		voicePack.installed = false;
		voicePack.installing = false;
		voicePack.error = null;
	});

	const englishReply = (id: string) =>
		mountDom('MessageBubble', {
			message: msg({ id, body_md: 'Hello there.' }),
			html: renderBody('Hello there.'),
		});

	it('offers the download with its size and the on-device hint', () => {
		const mounted = englishReply('v1');
		const btn = buttonByText(mounted.el, 'Download English voice (94 MB)');
		expect(btn?.title).toBe('Speech is generated on this device');
		mounted.unmount();
	});

	it('starts the install when the download button is clicked', () => {
		const install = jest.spyOn(voicePack, 'install').mockResolvedValue();
		const mounted = englishReply('v2');
		buttonByText(mounted.el, 'Download English voice (94 MB)')!.click();
		expect(install).toHaveBeenCalledTimes(1);
		install.mockRestore();
		mounted.unmount();
	});

	it('shows the download percentage and cancels a running download', async () => {
		const cancel = jest.spyOn(voicePack, 'cancel').mockResolvedValue();
		voicePack.installing = true;
		voicePack.progress = 0.42;
		const mounted = englishReply('v3');
		expect(voiceBox(mounted.el)?.textContent).toContain('Downloading 42%');
		buttonByText(mounted.el, 'Cancel')!.click();
		expect(cancel).toHaveBeenCalledTimes(1);
		cancel.mockRestore();
		mounted.unmount();
	});

	it('shows a short error and offers the download again', async () => {
		const install = jest.spyOn(voicePack, 'install').mockResolvedValue();
		voicePack.error = 'disk full';
		const mounted = englishReply('v4');
		expect(voiceBox(mounted.el)?.textContent).toContain(
			'English voice download failed',
		);
		expect(voiceBox(mounted.el)?.textContent).not.toContain('disk full');
		buttonByText(mounted.el, 'Download English voice (94 MB)')!.click();
		expect(install).toHaveBeenCalledTimes(1);
		install.mockRestore();
		mounted.unmount();
	});

	it('offers removal once the voice is installed', () => {
		const remove = jest.spyOn(voicePack, 'remove').mockResolvedValue();
		voicePack.installed = true;
		const mounted = englishReply('v5');
		expect(
			buttonByText(mounted.el, 'Download English voice (94 MB)'),
		).toBeUndefined();
		buttonByText(mounted.el, 'Remove English voice')!.click();
		expect(remove).toHaveBeenCalledTimes(1);
		remove.mockRestore();
		mounted.unmount();
	});

	it('is hidden for Russian replies, owner messages, decisions and unsupported platforms', () => {
		const ru = mountDom('MessageBubble', {
			message: msg({ id: 'r1', body_md: 'Привет.' }),
			html: renderBody('Привет.'),
		});
		expect(voiceBox(ru.el)).toBeNull();
		ru.unmount();

		const owner = mountDom('MessageBubble', {
			message: msg({
				id: 'r2',
				author: { kind: 'owner', id: 'me' },
				body_md: 'mine',
			}),
			html: renderBody('mine'),
		});
		expect(voiceBox(owner.el)).toBeNull();
		owner.unmount();

		const decision = mountDom('MessageBubble', {
			message: msg({ id: 'r3', kind: 'decision', body_md: 'Pick one' }),
		});
		expect(voiceBox(decision.el)).toBeNull();
		decision.unmount();

		voicePack.supported = false;
		const unsupported = englishReply('r4');
		expect(voiceBox(unsupported.el)).toBeNull();
		unsupported.unmount();
	});

	it('asks for the voice pack status once, not once per bubble', () => {
		const a = englishReply('s1');
		const b = englishReply('s2');
		const c = englishReply('s3');
		expect(refresh).toHaveBeenCalledTimes(1);
		a.unmount();
		b.unmount();
		c.unmount();
	});
});

describe('MessageBubble Russian voice control', () => {
	const MB = 61 * 1024 * 1024;
	const ru = voicePacks.ru;
	const box = (el: HTMLElement) => el.querySelector<HTMLElement>('.voice');
	const byText = (el: HTMLElement, text: string) =>
		Array.from(el.querySelectorAll<HTMLButtonElement>('.voice button')).find(
			(b) => b.textContent?.trim() === text,
		);
	const russianReply = (id: string) =>
		mountDom('MessageBubble', {
			message: msg({ id, body_md: 'Привет, как дела?' }),
			html: renderBody('Привет, как дела?'),
		});
	let refreshRu: jest.SpyInstance;
	let refreshEn: jest.SpyInstance;

	beforeEach(() => {
		ru.supported = true;
		ru.installed = false;
		ru.installing = false;
		ru.progress = 0;
		ru.error = null;
		ru.sizeBytes = MB;
		ru.statusRequested = false;
		voicePacks.en.statusRequested = false;
		refreshRu = jest.spyOn(ru, 'refresh').mockResolvedValue();
		refreshEn = jest.spyOn(voicePacks.en, 'refresh').mockResolvedValue();
	});

	afterEach(() => {
		refreshRu.mockRestore();
		refreshEn.mockRestore();
		ru.supported = false;
		ru.installed = false;
		ru.installing = false;
		ru.error = null;
	});

	it('offers the Russian download and installs only the Russian pack', () => {
		const install = jest.spyOn(ru, 'install').mockResolvedValue();
		const installEn = jest.spyOn(voicePacks.en, 'install').mockResolvedValue();
		const mounted = russianReply('ru1');
		expect(refreshRu).toHaveBeenCalledTimes(1);
		expect(refreshEn).not.toHaveBeenCalled();
		byText(mounted.el, 'Download Russian voice (61 MB)')!.click();
		expect(install).toHaveBeenCalledTimes(1);
		expect(installEn).not.toHaveBeenCalled();
		install.mockRestore();
		installEn.mockRestore();
		mounted.unmount();
	});

	it('shows Russian progress with cancel and removal', () => {
		const cancel = jest.spyOn(ru, 'cancel').mockResolvedValue();
		ru.installing = true;
		ru.progress = 0.5;
		const downloading = russianReply('ru2');
		expect(box(downloading.el)?.textContent).toContain('Downloading 50%');
		byText(downloading.el, 'Cancel')!.click();
		expect(cancel).toHaveBeenCalledTimes(1);
		cancel.mockRestore();
		downloading.unmount();

		const remove = jest.spyOn(ru, 'remove').mockResolvedValue();
		ru.installing = false;
		ru.installed = true;
		const installed = russianReply('ru3');
		byText(installed.el, 'Remove Russian voice')!.click();
		expect(remove).toHaveBeenCalledTimes(1);
		remove.mockRestore();
		installed.unmount();
	});

	it('shows a Russian download failure', () => {
		ru.error = 'disk full';
		const mounted = russianReply('ru4');
		expect(box(mounted.el)?.textContent).toContain(
			'Russian voice download failed',
		);
		mounted.unmount();
	});

	it('is hidden when the Russian pack is unsupported', () => {
		ru.supported = false;
		const mounted = russianReply('ru5');
		expect(box(mounted.el)).toBeNull();
		mounted.unmount();
	});
});

describe('MessageBubble external voice failure', () => {
	afterEach(() => {
		setEngine(speechSynthesisEngine);
		externalVoice.health = null;
	});

	it('shows the failure reason in the Speak button tooltip', async () => {
		externalVoice.health = {
			configured: true,
			mode: 'http',
			healthy: false,
			lastError: 'connection refused',
		};
		const fake: TtsEngine = {
			id: 'fake',
			isAvailable: () => true,
			speak: (_c, _l, hooks) => hooks.onError('external-error'),
			stop: jest.fn(),
		};
		setEngine(fake);
		const m = msg({ id: 'ext1', body_md: 'Hello there.' });
		const mounted = mountDom('MessageBubble', {
			message: m,
			html: renderBody(m.body_md),
		});
		speaker.toggle(m.id, m.body_md);
		await nextTick();
		expect(speaker.error).toBe('external-error');
		const btn = mounted.el.querySelector<HTMLButtonElement>('.macts button');
		expect(btn?.getAttribute('title')).toBe(
			'External voice failed: connection refused',
		);
		speaker.stop();
		mounted.unmount();
	});
});
