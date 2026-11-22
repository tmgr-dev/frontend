import type { Author, ChatMessage, RoomInfo } from '../types';

const AGENT_COLORS: Record<string, string> = {
	claude: '#c77b2d',
	codex: '#4a9266',
	opencode: '#608ab5',
	agy: '#857ac4',
};

export const agentColor = (agent?: string): string =>
	(agent && AGENT_COLORS[agent]) || '#888';

export const isManagerAuthor = (author: Author): boolean =>
	author.kind === 'manager';

export const avatarLetter = (agent?: string, manager = false): string =>
	manager ? 'M' : (agent?.[0] ?? '?').toUpperCase();

export const roomAgent = (info: RoomInfo): string | undefined =>
	info.room.kind === 'manager' ? undefined : info.agent;

export const formatContext = (tokens?: number): string =>
	tokens === undefined ? '' : `${Math.round(tokens / 1000)}k`;

export const contextLevel = (pct?: number): '' | 'warn' | 'crit' =>
	pct === undefined ? '' : pct > 90 ? 'crit' : pct > 80 ? 'warn' : '';

export const plainPreview = (md: string, max = 60): string =>
	md
		.replace(/[#*`|>]/g, '')
		.replace(/(^|\s)-\s/g, '$1')
		.replace(/\s+/g, ' ')
		.trim()
		.slice(0, max);

export const formatClock = (tsMs: number): string => {
	const d = new Date(tsMs);
	return `${String(d.getHours()).padStart(2, '0')}:${String(
		d.getMinutes(),
	).padStart(2, '0')}`;
};

export const dayKey = (tsMs: number): string => {
	const d = new Date(tsMs);
	return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
};

export const formatDay = (
	tsMs: number,
	now: number,
	labels: { today: string; yesterday: string },
): string => {
	if (dayKey(tsMs) === dayKey(now)) return labels.today;
	if (dayKey(tsMs) === dayKey(now - 86_400_000)) return labels.yesterday;
	return new Date(tsMs).toLocaleDateString(undefined, {
		month: 'short',
		day: 'numeric',
	});
};

const isTextual = (m: ChatMessage): boolean =>
	m.author.kind !== 'system' && (m.kind === 'text' || m.kind === 'decision');

const GROUP_WINDOW_MS = 5 * 60_000;

export const isGrouped = (
	prev: ChatMessage | undefined,
	cur: ChatMessage,
): boolean =>
	!!prev &&
	isTextual(prev) &&
	isTextual(cur) &&
	prev.author.kind === cur.author.kind &&
	prev.author.id === cur.author.id &&
	!!prev.user_turn === !!cur.user_turn &&
	dayKey(prev.ts_ms) === dayKey(cur.ts_ms) &&
	cur.ts_ms - prev.ts_ms < GROUP_WINDOW_MS;

export const truncate = (text: string, max: number): string =>
	text.length > max ? `${text.slice(0, max)}…` : text;

export const highlightParts = (
	text: string,
	query: string,
): { text: string; hit: boolean }[] => {
	const needle = query.trim();
	if (!needle) return [{ text, hit: false }];
	const re = new RegExp(
		`(${needle.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`,
		'ig',
	);
	return text
		.split(re)
		.filter((part) => part !== '')
		.map((part) => ({
			text: part,
			hit: part.toLowerCase() === needle.toLowerCase(),
		}));
};

export const snippetAround = (text: string, query: string): string => {
	const flat = text
		.replace(/[#*`|]/g, '')
		.replace(/\s+/g, ' ')
		.trim();
	const at = Math.max(
		0,
		flat.toLowerCase().indexOf(query.trim().toLowerCase()) - 28,
	);
	return (at ? '…' : '') + flat.slice(at, at + 110);
};

export const ICONS: Record<
	string,
	{ body: string; fill?: boolean; sw?: number }
> = {
	lock: {
		body: '<rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>',
	},
	gear: {
		body: '<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M19.1 4.9L17 7M7 17l-2.1 2.1"/>',
	},
	check: { body: '<path d="M5 12l5 5 9-10"/>', sw: 2.5 },
	x: { body: '<path d="M6 6l12 12M18 6L6 18"/>', sw: 2.5 },
	bolt: { body: '<path d="M13 2L4 14h6l-1 8 9-12h-6z"/>', fill: true },
	hand: {
		body: '<circle cx="12" cy="12" r="9"/><path d="M12 7v6M12 16.5v.5"/>',
	},
	dot: { body: '<circle cx="12" cy="12" r="5"/>', fill: true },
	send: { body: '<path d="M3 11l18-8-8 18-2-8z"/>', fill: true },
	file: {
		body: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
	},
	clip: {
		body: '<path d="M20 11l-8.5 8.5a5 5 0 0 1-7-7L13 4a3.5 3.5 0 0 1 5 5l-8.5 8.5a2 2 0 0 1-3-3L14 7"/>',
	},
	copy: {
		body: '<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V6a2 2 0 0 1 2-2h8"/>',
	},
	reply: {
		body: '<path d="M10 8L4 13l6 5v-3.5c5 0 8 1 10 4-.5-5-3.5-8.5-10-8.5z"/>',
	},
	speak: {
		body: '<path d="M11 5L6 9H3v6h3l5 4z"/><path d="M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13"/>',
	},
	stop: {
		body: '<rect x="6" y="6" width="12" height="12" rx="2"/>',
	},
	search: { body: '<circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/>' },
	term: {
		body: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 10l3 2-3 2M12 15h5"/>',
	},
};

export const contextFull = (tokens?: number, pct?: number): string => {
	const used = formatContext(tokens);
	if (pct === undefined || tokens === undefined || pct <= 0) return used;
	return `${used} / ${Math.round(tokens / (pct / 100) / 1000)}k · ${pct}%`;
};

export const formatSize = (bytes: number): string =>
	bytes < 1024
		? `${bytes} B`
		: bytes < 1024 * 1024
			? `${Math.round(bytes / 1024)} KB`
			: `${(bytes / 1024 / 1024).toFixed(1)} MB`;

export const authorLabel = (
	author: Author,
	t: (key: string) => string,
	userTurn?: boolean,
): string => {
	if (author.kind === 'owner' || userTurn) return t('manager.messages.you');
	if (author.kind === 'manager') return t('manager.messages.manager');
	if (author.kind === 'session') return author.agent ?? author.id;
	return t('manager.messages.system');
};
