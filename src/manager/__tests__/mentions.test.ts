import {
	firstEnabled,
	injectGate,
	isMentionStateRow,
	latestMentions,
	mentionCandidates,
	mentionHandle,
	mentionQuery,
	mentionRole,
	mentionSupported,
	nextEnabled,
	stripMention,
} from '../mentions';
import {
	roomKey,
	type ChatMessage,
	type MentionRef,
	type MentionState,
	type RoomInfo,
	type RoomRef,
} from '../types';

const sess = (id: string): RoomRef => ({
	kind: 'session',
	machine: 'mac',
	session: id,
});
const MGR: RoomRef = { kind: 'manager', machine: 'mac' };

const room = (
	id: string,
	title: string,
	mention: RoomInfo['mention'] = { ok: true },
	extra: Partial<RoomInfo> = {},
): RoomInfo => ({
	room: sess(id),
	title,
	agent: 'claude',
	mention,
	...extra,
});

describe('mention caps gate', () => {
	it('is on only when the runner announces chat.mention.v1', () => {
		expect(mentionSupported(['chat.v1', 'chat.mention.v1'])).toBe(true);
		expect(mentionSupported(['chat.v1'])).toBe(false);
		expect(mentionSupported([])).toBe(false);
	});
});

describe('mentionQuery', () => {
	it('opens only for a leading @token', () => {
		expect(mentionQuery('@')).toBe('');
		expect(mentionQuery('@Build')).toBe('build');
		expect(mentionQuery('hi @build')).toBeNull();
		expect(mentionQuery('@build fix')).toBeNull();
		expect(mentionQuery('/stop')).toBeNull();
	});
});

describe('mentionCandidates', () => {
	const rooms: RoomInfo[] = [
		room('s_b', 'build-fix'),
		room('s_a', 'api work'),
		room('s_x', 'exited one', { ok: false, reason: 'Exited' }),
		{ room: sess('s_old'), title: 'old runner' },
		room('s_far', 'far', { ok: true }, { local: false }),
		room('s_self', 'me'),
		{ room: MGR, title: 'Manager', mention: { ok: true } },
	];

	it('lists local rooms that carry a verdict, never the active one', () => {
		const items = mentionCandidates(rooms, roomKey(sess('s_self')), '');
		expect(items.map((i) => i.sessionId)).toEqual(['s_a', 's_b', '', 's_x']);
		expect(items.find((i) => i.sessionId === 's_x')).toMatchObject({
			ok: false,
			reason: 'Exited',
		});
	});

	it('puts enabled rows first and keeps disabled rows with their reason', () => {
		const items = mentionCandidates(rooms, null, '');
		const firstOff = items.findIndex((i) => !i.ok);
		expect(items.slice(firstOff).every((i) => !i.ok)).toBe(true);
	});

	it('filters by handle or session id prefix', () => {
		expect(
			mentionCandidates(rooms, null, 'build').map((i) => i.sessionId),
		).toEqual(['s_b']);
		expect(
			mentionCandidates(rooms, null, 's_a').map((i) => i.sessionId),
		).toEqual(['s_a']);
		expect(mentionCandidates(rooms, null, 'api-w')).toHaveLength(1);
	});

	it('flags equal titles as ambiguous, ignoring case', () => {
		const items = mentionCandidates(
			[room('s1', 'Fix'), room('s2', 'fix'), room('s3', 'other')],
			null,
			'',
		);
		expect(
			Object.fromEntries(items.map((i) => [i.sessionId, i.ambiguous])),
		).toEqual({ s1: true, s2: true, s3: false });
	});

	it('carries the risky flag', () => {
		const [item] = mentionCandidates(
			[room('s1', 'bypass', { ok: true, risky: true })],
			null,
			'',
		);
		expect(item.risky).toBe(true);
	});
});

describe('menu navigation skips disabled rows', () => {
	const items = mentionCandidates(
		[
			room('a', 'a'),
			room('b', 'b', { ok: false, reason: 'No transcript' }),
			room('c', 'c'),
		],
		null,
		'',
	);
	it('starts on the first enabled row and wraps over enabled rows', () => {
		expect(items.map((i) => i.ok)).toEqual([true, true, false]);
		expect(firstEnabled(items)).toBe(0);
		expect(nextEnabled(items, 0, 1)).toBe(1);
		expect(nextEnabled(items, 1, 1)).toBe(0);
		expect(nextEnabled(items, 0, -1)).toBe(1);
	});

	it('stays put when nothing is enabled', () => {
		const off = mentionCandidates([room('a', 'a', { ok: false })], null, '');
		expect(nextEnabled(off, 0, 1)).toBe(0);
		expect(firstEnabled(off)).toBe(0);
	});
});

describe('handle and body', () => {
	it('turns spaces into dashes', () => {
		expect(mentionHandle(' Build  fix ')).toBe('Build-fix');
	});

	it('strips the leading handle and nothing else', () => {
		expect(stripMention('@build-fix what is up?', 'build-fix')).toBe(
			'what is up?',
		);
		expect(stripMention('@build-fix', 'build-fix')).toBe('');
		expect(stripMention('@build-fixer hi', 'build-fix')).toBeNull();
		expect(stripMention('hi @build-fix', 'build-fix')).toBeNull();
	});
});

const ref = (id: string, state: MentionState): MentionRef => ({
	id,
	target: sess('b'),
	target_title: 'build-fix',
	state,
	hop: 1,
});

const row = (
	id: string,
	ts: number,
	over: Partial<ChatMessage> = {},
): ChatMessage => ({
	v: 1,
	id,
	room: MGR,
	author: { kind: 'system', id: 'runner' },
	ts_ms: ts,
	kind: 'system',
	body_md: '',
	...over,
});

describe('latest state per mention id', () => {
	const q = row('m_q', 1, {
		author: { kind: 'owner', id: 'owner' },
		kind: 'text',
		mention: ref('m_q', 'queued'),
	});
	const sent = row('m_s', 2, { reply_to: 'm_q', mention: ref('m_q', 'sent') });
	const ans = row('m_a', 3, {
		reply_to: 'm_q',
		mention: ref('m_q', 'answering'),
	});

	it('picks the newest row, whatever the array order', () => {
		expect(latestMentions([ans, q, sent]).get('m_q')?.state).toBe('answering');
		expect(latestMentions([q]).get('m_q')?.state).toBe('queued');
	});

	it('breaks timestamp ties by id', () => {
		const a = row('m_1', 5, { reply_to: 'm_q', mention: ref('m_q', 'busy') });
		const b = row('m_2', 5, {
			reply_to: 'm_q',
			mention: ref('m_q', 'timeout'),
		});
		expect(latestMentions([b, a]).get('m_q')?.state).toBe('timeout');
	});

	it('tracks several mentions independently and ignores plain rows', () => {
		const other = row('m_o', 9, {
			author: { kind: 'owner', id: 'owner' },
			kind: 'text',
			mention: ref('m_o', 'queued'),
		});
		const plain = row('m_p', 10, { kind: 'text' });
		const map = latestMentions([q, ans, other, plain]);
		expect([...map.keys()].sort()).toEqual(['m_o', 'm_q']);
	});

	it('answered reply rows win over earlier states', () => {
		const reply = row('m_r', 4, {
			author: { kind: 'session', id: 'b' },
			kind: 'text',
			reply_to: 'm_q',
			mention: ref('m_q', 'answered'),
		});
		expect(latestMentions([q, sent, ans, reply]).get('m_q')?.state).toBe(
			'answered',
		);
	});

	it('classifies rows by role', () => {
		const reply = row('m_r', 4, {
			author: { kind: 'session', id: 'b' },
			kind: 'text',
			mention: ref('m_q', 'answered'),
		});
		expect(mentionRole(q)).toBe('question');
		expect(mentionRole(sent)).toBe('state');
		expect(mentionRole(reply)).toBe('reply');
		expect(mentionRole(row('x', 1))).toBeNull();
		expect(isMentionStateRow(sent)).toBe(true);
		expect(isMentionStateRow(q)).toBe(false);
		expect(isMentionStateRow(reply)).toBe(false);
	});
});

describe('injectGate: Add to context is local only', () => {
	const reply = (r: RoomRef = MGR): ChatMessage =>
		row('m_r', 4, {
			room: r,
			author: { kind: 'session', id: 'b' },
			kind: 'text',
			mention: ref('m_q', 'answered'),
		});
	const origin = (over: Partial<RoomInfo> = {}): RoomInfo => ({
		room: MGR,
		title: 'Manager',
		...over,
	});

	it('is hidden without an origin, on relay rooms and on non-reply rows', () => {
		expect(injectGate(undefined, reply(), true).show).toBe(false);
		expect(injectGate(origin({ local: false }), reply(), true).show).toBe(
			false,
		);
		expect(
			injectGate(origin(), row('s', 1, { mention: ref('m_q', 'sent') }), true)
				.show,
		).toBe(false);
		expect(injectGate(origin({ room: sess('zz') }), reply(), true).show).toBe(
			false,
		);
	});

	it('shows for a local session origin without confirmation', () => {
		const r = reply(sess('a'));
		expect(injectGate(origin({ room: sess('a') }), r, true)).toEqual({
			show: true,
			refused: false,
			confirm: false,
		});
	});

	it('needs confirmation for a manager or risky origin', () => {
		expect(injectGate(origin(), reply(), true).confirm).toBe(true);
		const r = reply(sess('a'));
		expect(
			injectGate(
				origin({ room: sess('a'), mention: { ok: true, risky: true } }),
				r,
				true,
			).confirm,
		).toBe(true);
	});

	it('is hidden without the capability, whatever the origin', () => {
		expect(injectGate(origin(), reply(), false).show).toBe(false);
		expect(
			injectGate(origin({ unrestricted: true }), reply(), false).show,
		).toBe(false);
	});

	it('refuses an unrestricted origin', () => {
		const g = injectGate(origin({ unrestricted: true }), reply(), true);
		expect(g).toEqual({ show: true, refused: true, confirm: false });
	});
});
