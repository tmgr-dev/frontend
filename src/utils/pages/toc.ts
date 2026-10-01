export interface TocEntry {
	text: string;
	occurrence: number;
}

const HEADING = /^##\s+(.+?)\s*#*\s*$/;
const FENCE = /^ {0,3}(`{3,}|~{3,})/;

const plainText = (value: string): string =>
	value
		.replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
		.replace(/[*_`~]/g, '')
		.trim();

export const extractToc = (body: string): TocEntry[] => {
	const entries: TocEntry[] = [];
	const seen = new Map<string, number>();
	let fence: string | null = null;
	for (const line of body.split(/\r?\n/)) {
		const fenceMatch = FENCE.exec(line);
		if (fenceMatch) {
			const mark = fenceMatch[1][0];
			if (fence === null) fence = mark;
			else if (fence === mark) fence = null;
			continue;
		}
		if (fence !== null) continue;
		const match = HEADING.exec(line);
		if (!match) continue;
		const text = plainText(match[1]);
		if (!text) continue;
		const occurrence = seen.get(text) ?? 0;
		seen.set(text, occurrence + 1);
		entries.push({ text, occurrence });
	}
	return entries;
};
