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
import {
	roomKey,
	type ChatMessage,
	type RoomInfo,
	type SlashCommand,
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

	it('ChatHeader shows the Unrestricted badge only for unrestricted rooms', async () => {
		const on = await render('ChatHeader', {
			info: { ...rooms[0], unrestricted: true },
		});
		expect(on).toContain('Unrestricted');
		expect(on).toContain('type into bypass sessions');
		const off = await render('ChatHeader', { info: rooms[0] });
		expect(off).not.toContain('Unrestricted');
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
		const state = reactive({ messages: initial, roomKey: 'r1' });
		const shownRooms = new Set<string>();
		const el = document.createElement('div');
		document.body.appendChild(el);
		const app = createApp({
			render: () =>
				h(load('MessageList'), {
					messages: state.messages,
					roomKey: state.roomKey,
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
