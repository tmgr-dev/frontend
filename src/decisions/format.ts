import type { CardKind, CardOption, CardView } from './types';

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
	const text = `${url.host}${url.pathname}`.replace(/\/$/, '');
	return text.length > 30 ? `${text.slice(0, 29)}…` : text;
};

export const recommendedOption = (card: CardView): CardOption | null =>
	card.options.find((option) => option.recommended) ?? null;

export const sourceLabel = (card: CardView): string => {
	const { agent, title } = card.source;
	return title ? `${agent} · ${title}` : agent;
};
