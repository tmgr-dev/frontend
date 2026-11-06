import type { CardState, CardView } from './types';

export interface QueueGroup {
	key: string;
	label: string;
	agent: string;
	cards: CardView[];
}

export interface QueueProgress {
	done: number;
	total: number;
	pending: number;
	percent: number;
}

const PENDING: CardState[] = ['open', 'deferred'];

export const isPending = (card: CardView): boolean =>
	PENDING.includes(card.state);

const baseName = (path: string): string => {
	const parts = path.split(/[\\/]/).filter(Boolean);
	return parts.length ? parts[parts.length - 1] : path;
};

export const groupKey = (card: CardView): string => {
	const { cwd, session, agent } = card.source;
	if (cwd) return `cwd:${cwd}`;
	if (session) return `session:${agent}:${session}`;
	return `agent:${agent}`;
};

export const groupLabel = (card: CardView): string => {
	const { cwd, title, agent } = card.source;
	if (cwd) return baseName(cwd);
	return title || agent;
};

const rank = (card: CardView): [number, number, number] => [
	card.state === 'deferred' ? 1 : 0,
	card.blocking ? 0 : 1,
	card.created_ms,
];

export const compareCards = (a: CardView, b: CardView): number => {
	const ra = rank(a);
	const rb = rank(b);
	for (let i = 0; i < ra.length; i += 1) {
		if (ra[i] !== rb[i]) return ra[i] - rb[i];
	}
	return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
};

export const visibleCards = (
	cards: CardView[],
	resolvedHere: ReadonlySet<string>,
): CardView[] =>
	cards.filter((card) => isPending(card) || resolvedHere.has(card.id));

const searchText = (card: CardView): string =>
	[
		card.title,
		card.context,
		card.question,
		card.links.task,
		...(card.links.items ?? []).flatMap((item) => [item.label, item.url]),
	]
		.filter(Boolean)
		.join('\n')
		.toLowerCase();

export const filterCards = (cards: CardView[], query: string): CardView[] => {
	const needle = query.trim().toLowerCase();
	if (!needle) return cards;
	return cards.filter((card) => searchText(card).includes(needle));
};

export const buildQueue = (cards: CardView[]): QueueGroup[] => {
	const map = new Map<string, QueueGroup>();
	for (const card of cards) {
		const key = groupKey(card);
		let group = map.get(key);
		if (!group) {
			group = {
				key,
				label: groupLabel(card),
				agent: card.source.agent,
				cards: [],
			};
			map.set(key, group);
		}
		group.cards.push(card);
	}
	const groups = [...map.values()];
	for (const group of groups) {
		group.cards.sort((a, b) => {
			const doneA = isPending(a) ? 0 : 1;
			const doneB = isPending(b) ? 0 : 1;
			return doneA - doneB || compareCards(a, b);
		});
	}
	groups.sort((a, b) => {
		const first = (g: QueueGroup) => g.cards.find(isPending) ?? g.cards[0];
		const ga = first(a);
		const gb = first(b);
		const donea = isPending(ga) ? 0 : 1;
		const doneb = isPending(gb) ? 0 : 1;
		return donea - doneb || compareCards(ga, gb);
	});
	return groups;
};

export const flattenQueue = (groups: QueueGroup[]): CardView[] =>
	groups.flatMap((group) => group.cards);

export const queueProgress = (cards: CardView[]): QueueProgress => {
	const total = cards.length;
	const pending = cards.filter(isPending).length;
	const done = total - pending;
	return {
		done,
		total,
		pending,
		percent: total === 0 ? 0 : Math.round((done / total) * 100),
	};
};

export const nextCardId = (
	order: CardView[],
	currentId: string | null,
): string | null => {
	const start = order.findIndex((card) => card.id === currentId);
	const rotated =
		start < 0 ? order : [...order.slice(start + 1), ...order.slice(0, start)];
	const candidates = rotated.filter(
		(card) => card.id !== currentId && isPending(card),
	);
	return (
		(candidates.find((card) => card.state === 'open') ?? candidates[0] ?? null)
			?.id ?? null
	);
};

export const initialCardId = (order: CardView[]): string | null =>
	(order.find((card) => card.state === 'open') ?? order.find(isPending))?.id ??
	null;

export const stepCardId = (
	order: CardView[],
	currentId: string | null,
	delta: 1 | -1,
): string | null => {
	if (order.length === 0) return null;
	const at = order.findIndex((card) => card.id === currentId);
	if (at < 0) return order[0].id;
	return order[(at + delta + order.length) % order.length].id;
};
