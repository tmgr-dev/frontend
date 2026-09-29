export interface ChangelogSection {
	version: string;
	date: string;
	body: string;
}

export interface WhatsNewContext {
	lastSeen: string | null | undefined;
	current: string;
	hasPriorData: boolean;
}

const versionParts = (version: string): number[] =>
	version
		.trim()
		.replace(/^v/i, '')
		.split('.')
		.map((part) => parseInt(part, 10) || 0);

export const compareVersions = (a: string, b: string): number => {
	const left = versionParts(a);
	const right = versionParts(b);
	const length = Math.max(left.length, right.length, 3);
	for (let i = 0; i < length; i++) {
		const diff = (left[i] ?? 0) - (right[i] ?? 0);
		if (diff !== 0) return diff > 0 ? 1 : -1;
	}
	return 0;
};

const HEADING = /^##\s+v?(\d+(?:\.\d+)*)\s*(?:[—–-]+\s*(.*?))?\s*$/;
const LIST_ITEM = /^\s*(?:[-*+]|\d+[.)])\s/;

const joinWrappedLines = (lines: string[]): string => {
	const out: string[] = [];
	let joinable = false;
	for (const line of lines) {
		if (line.trim() === '') {
			out.push('');
			joinable = false;
		} else if (joinable && /^\s/.test(line) && !LIST_ITEM.test(line)) {
			out[out.length - 1] += ` ${line.trim()}`;
		} else {
			out.push(line.replace(/\s+$/, ''));
			joinable = LIST_ITEM.test(line);
		}
	}
	return out.join('\n').trim();
};

export const parseChangelog = (markdown: string): ChangelogSection[] => {
	const sections: ChangelogSection[] = [];
	let current: { version: string; date: string; lines: string[] } | null = null;
	for (const line of (markdown || '').split(/\r?\n/)) {
		if (/^##(?!#)/.test(line)) {
			const match = HEADING.exec(line);
			if (current) sections.push(finish(current));
			current = match
				? { version: match[1], date: (match[2] || '').trim(), lines: [] }
				: null;
		} else if (current) {
			current.lines.push(line);
		}
	}
	if (current) sections.push(finish(current));
	return sections;
};

const finish = (raw: {
	version: string;
	date: string;
	lines: string[];
}): ChangelogSection => ({
	version: raw.version,
	date: raw.date,
	body: joinWrappedLines(raw.lines),
});

const newestFirst = (sections: ChangelogSection[]): ChangelogSection[] =>
	[...sections].sort((a, b) => compareVersions(b.version, a.version));

export const whatsNewSections = (
	sections: ChangelogSection[],
	{ lastSeen, current, hasPriorData }: WhatsNewContext,
): ChangelogSection[] => {
	if (!lastSeen) {
		if (!hasPriorData) return [];
		return sections.filter((s) => compareVersions(s.version, current) === 0);
	}
	return newestFirst(
		sections.filter(
			(s) =>
				compareVersions(s.version, lastSeen) > 0 &&
				compareVersions(s.version, current) <= 0,
		),
	);
};

export const recentSections = (
	sections: ChangelogSection[],
	current: string,
	limit = 10,
): ChangelogSection[] =>
	newestFirst(
		sections.filter((s) => compareVersions(s.version, current) <= 0),
	).slice(0, limit);
