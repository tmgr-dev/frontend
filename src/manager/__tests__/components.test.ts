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
	if (id === '../types') return require('../types');
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
