import { currentLocale, i18n, t } from '@/i18n';
import type {
	CardKind,
	CardLinkKind,
	CardLinks,
	CardOption,
	CardView,
} from './types';

export const kindLabel = (kind: CardKind): string => {
	const key = `decisions.kind.${kind}`;
	return i18n.global.te(key) ? t(key) : kind;
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
	if (pull) return t('decisions.link.pr', { n: pull[1] });
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

const LINK_SECTIONS: CardLinkKind[] = ['pr', 'commit', 'tag', 'task', 'url'];

export const linkSections = (links: CardLinks): LinkSection[] => {
	const seen = new Set<string>();
	const byKind = new Map<CardLinkKind, LinkEntry[]>();
	const add = (kind: CardLinkKind, url: string, label?: string) => {
		if (!url || seen.has(url)) return;
		seen.add(url);
		const target = LINK_SECTIONS.includes(kind) ? kind : 'url';
		const list = byKind.get(target) ?? [];
		list.push({ url, label: label || linkLabel(url) });
		byKind.set(target, list);
	};
	for (const item of links.items ?? []) add(item.kind, item.url, item.label);
	if (links.pr) add('pr', links.pr);
	if (links.task) add('task', links.task);
	return LINK_SECTIONS.flatMap((kind) => {
		const entries = byKind.get(kind);
		return entries
			? [{ title: t(`decisions.link.sections.${kind}`), entries }]
			: [];
	});
};

export const updatedLabel = (ms: number): string =>
	new Date(ms).toLocaleTimeString(currentLocale(), {
		hour: '2-digit',
		minute: '2-digit',
	});
