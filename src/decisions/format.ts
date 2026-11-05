import type {
	CardKind,
	CardLinkKind,
	CardLinks,
	CardOption,
	CardView,
} from './types';

export const KIND_LABELS: Record<CardKind, string> = {
	decision: 'Decision',
	review: 'Review',
	release: 'Release',
	check: 'Check',
	question: 'Question',
	permission: 'Permission',
	stopped: 'Finished',
};

export const isHttpUrl = (value: string): boolean => {
	try {
		const url = new URL(value);
		return url.protocol === 'http:' || url.protocol === 'https:';
	} catch {
		return false;
	}
};

export const linkLabel = (value: string): string => {
	if (!isHttpUrl(value)) return value;
	const url = new URL(value);
	const pull = /\/pull\/(\d+)/.exec(url.pathname);
	if (pull) return `PR #${pull[1]}`;
	const issue = /\/issues\/(\d+)/.exec(url.pathname);
	if (issue) return `#${issue[1]}`;
	const tag = /\/releases\/tag\/(.+?)\/?$/.exec(url.pathname);
	if (tag) return tag[1];
	const text = `${url.host}${url.pathname}`.replace(/\/$/, '');
	return text.length > 30 ? `${text.slice(0, 29)}…` : text;
};

export const recommendedOption = (card: CardView): CardOption | null =>
	card.options.find((option) => option.recommended) ?? null;

export const sourceLabel = (card: CardView): string => {
	const { agent, title } = card.source;
	return title ? `${agent} · ${title}` : agent;
};

export interface LinkEntry {
	url: string;
	label: string;
}

export interface LinkSection {
	title: string;
	entries: LinkEntry[];
}

const LINK_SECTIONS: { kind: CardLinkKind; title: string }[] = [
	{ kind: 'pr', title: 'PRs' },
	{ kind: 'commit', title: 'Commits' },
	{ kind: 'tag', title: 'Tags' },
	{ kind: 'task', title: 'Task' },
	{ kind: 'url', title: 'Other' },
];

export const linkSections = (links: CardLinks): LinkSection[] => {
	const seen = new Set<string>();
	const byKind = new Map<CardLinkKind, LinkEntry[]>();
	const add = (kind: CardLinkKind, url: string, label?: string) => {
		if (!url || seen.has(url)) return;
		seen.add(url);
		const target = LINK_SECTIONS.some((s) => s.kind === kind) ? kind : 'url';
		const list = byKind.get(target) ?? [];
		list.push({ url, label: label || linkLabel(url) });
		byKind.set(target, list);
	};
	for (const item of links.items ?? []) add(item.kind, item.url, item.label);
	if (links.pr) add('pr', links.pr);
	if (links.task) add('task', links.task);
	return LINK_SECTIONS.flatMap(({ kind, title }) => {
		const entries = byKind.get(kind);
		return entries ? [{ title, entries }] : [];
	});
};

export const updatedLabel = (ms: number): string =>
	new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
