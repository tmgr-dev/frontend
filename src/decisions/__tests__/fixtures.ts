import type { CardView } from '../types';

export const makeCard = (over: Partial<CardView> = {}): CardView => ({
	id: 'c_1',
	created_ms: 1000,
	state: 'open',
	session_gone: false,
	can_reply: true,
	messages: [],
	source: { agent: 'claude-code', cwd: '/work/alpha' },
	kind: 'decision',
	blocking: false,
	title: 'Card',
	facts: [],
	options: [],
	checks: [],
	diffs: [],
	patches: [],
	links: {},
	reply_via: 'mcp',
	...over,
});
