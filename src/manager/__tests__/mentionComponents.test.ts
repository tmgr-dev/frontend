/**
 * @jest-environment jsdom
 */
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createSSRApp, type Component } from 'vue';
import { createI18n } from 'vue-i18n';
import { renderToString } from 'vue/server-renderer';
import manager from '../../locales/en/manager.json';
import { mentionCandidates } from '../mentions';
import type { MentionRef, MentionState } from '../types';
import { type ChatMessage } from '../types';

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

const sess = { kind: 'session' as const, machine: 'mac', session: 's_b' };
const mgr = { kind: 'manager' as const, machine: 'mac' };

const info = (title: string, id: string, mention: object) => ({
	room: { ...sess, session: id },
	title,
	agent: 'claude',
	mention,
});

const ref = (state: MentionState): MentionRef => ({
	id: 'm_q',
	target: sess,
	target_title: 'build-fix',
	state,
	hop: 1,
});

const row = (id: string, ts: number, over: Partial<ChatMessage>): ChatMessage =>
	({
		v: 1,
		id,
		room: mgr,
		author: { kind: 'system', id: 'runner' },
		ts_ms: ts,
		kind: 'system',
		body_md: '',
		...over,
	}) as ChatMessage;

describe('mention components', () => {
	it('MentionMenu shows the reason on disabled rows and ids for equal titles', async () => {
		const items = mentionCandidates(
			[
				info('fix', 's_1', { ok: true }),
				info('fix', 's_2', { ok: true }),
				info('gone', 's_3', { ok: false, reason: 'Exited' }),
				info('wild', 's_4', { ok: true, risky: true }),
			] as never,
			null,
			'',
		);
		const html = await render('MentionMenu', { items, activeIndex: 0 });
		expect(html.match(/role="option"/g)).toHaveLength(4);
		expect(html).toContain('Exited');
		expect(html).toMatch(/aria-disabled="true"/);
		expect(html).toContain('s_1');
		expect(html).toContain('s_2');
		expect(html).toContain('asks you first');
	});

	it('MentionReply shows the latest state under the question', async () => {
		const q = row('m_q', 1, {
			author: { kind: 'owner', id: 'owner' },
			kind: 'text',
			mention: ref('queued'),
		});
		const html = await render('MentionReply', {
			message: q,
			latest: ref('answering'),
		});
		expect(html).toContain('@build-fix is answering');
		expect(html).not.toContain('Add to context');
	});

	it('MentionReply shows the reason line and the no-answer label for a released timeout', async () => {
		const q = row('m_q', 1, {
			author: { kind: 'owner', id: 'owner' },
			kind: 'text',
			mention: ref('queued'),
		});
		const released = await render('MentionReply', {
			message: q,
			latest: {
				...ref('timeout'),
				reason: 'the session is at its prompt but no answer was captured',
			},
		});
		expect(released).toContain('No answer captured from @build-fix');
		expect(released).toContain('the session is at its prompt');
		const plain = await render('MentionReply', {
			message: q,
			latest: ref('timeout'),
		});
		expect(plain).toContain('No answer from @build-fix in 10 min');
		const busy = await render('MentionReply', {
			message: q,
			latest: { ...ref('busy'), reason: 'the queue is full' },
		});
		expect(busy).toContain('the queue is full');
	});

	it('MentionReply on a reply offers Add to context only for a local origin', async () => {
		const reply = row('m_r', 5, {
			author: { kind: 'session', id: 's_b' },
			kind: 'text',
			mention: ref('answered'),
		});
		const local = await render('MentionReply', {
			message: reply,
			latest: ref('answered'),
			supported: true,
			origin: { room: mgr, title: 'Manager' },
		});
		expect(local).toContain('Reply from @build-fix');
		expect(local).toContain('Add to context');
		expect(local).toContain('Open @build-fix');
		const unsupported = await render('MentionReply', {
			message: reply,
			latest: ref('answered'),
			origin: { room: mgr, title: 'Manager' },
		});
		expect(unsupported).not.toContain('Add to context');
		const remote = await render('MentionReply', {
			message: reply,
			latest: ref('answered'),
			supported: true,
			origin: { room: mgr, title: 'Manager', local: false },
		});
		expect(remote).not.toContain('Add to context');
		expect(remote).toContain('Open @build-fix');
		const locked = await render('MentionReply', {
			message: reply,
			latest: ref('answered'),
			supported: true,
			origin: { room: mgr, title: 'Manager', unrestricted: true },
		});
		expect(locked).toContain('Not available for unrestricted sessions yet');
		expect(locked).not.toContain('>Add to context<');
	});

	it('MessageList hides state rows and renders one status line per question', async () => {
		const q = row('m_q', 1, {
			author: { kind: 'owner', id: 'owner' },
			kind: 'text',
			body_md: 'is it green?',
			mention: ref('queued'),
		});
		const s1 = row('m_s1', 2, {
			reply_to: 'm_q',
			body_md: 'STATE-ONE',
			mention: ref('sent'),
		});
		const s2 = row('m_s2', 3, {
			reply_to: 'm_q',
			body_md: 'STATE-TWO',
			mention: ref('answering'),
		});
		const html = await render('MessageList', { messages: [q, s1, s2] });
		expect(html).not.toContain('STATE-ONE');
		expect(html).not.toContain('STATE-TWO');
		expect(html.match(/class="mention"/g)).toHaveLength(1);
		expect(html).toContain('@build-fix is answering');
	});
});
