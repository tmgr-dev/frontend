interface Projected {
	plain: string;
	map: number[];
}

interface LinkSpan {
	open: number;
	textStart: number;
	textEnd: number;
	close: number;
}

const MARKERS = ['**', '__', '~~', '`', '*', '_'];

const project = (body: string) => {
	let plain = '';
	const map: number[] = [];
	const links: LinkSpan[] = [];
	let i = 0;
	let lineStart = true;
	const push = (char: string, index: number) => {
		plain += char;
		map.push(index);
	};
	while (i < body.length) {
		const char = body[i];
		if (lineStart) {
			const prefix =
				/^[ \t]*(?:#{1,6}[ \t]+|>[ \t]?|[-*+][ \t]+(?:\[[ xX]\][ \t]+)?|\d+[.)][ \t]+)/.exec(
					body.slice(i, i + 40),
				);
			if (prefix && prefix[0].length) {
				i += prefix[0].length;
				lineStart = false;
				continue;
			}
		}
		lineStart = false;
		if (char === '\n') {
			push(' ', i);
			lineStart = true;
			i++;
			continue;
		}
		if (
			char === '\\' &&
			i + 1 < body.length &&
			/[\\`*_{}[\]()#+\-.!~>]/.test(body[i + 1])
		) {
			push(body[i + 1], i + 1);
			i += 2;
			continue;
		}
		if (char === '[') {
			const link = /^\[((?:\\.|[^\]\\\n])*)\]\(([^)\s]*)\)/.exec(body.slice(i));
			if (link) {
				const textStart = i + 1;
				const textEnd = textStart + link[1].length;
				links.push({
					open: i,
					textStart,
					textEnd,
					close: i + link[0].length,
				});
				let j = textStart;
				while (j < textEnd) {
					if (body[j] === '\\' && j + 1 < textEnd) j++;
					push(body[j], j);
					j++;
				}
				i += link[0].length;
				continue;
			}
		}
		const marker = MARKERS.find((m) => body.startsWith(m, i));
		if (marker) {
			i += marker.length;
			continue;
		}
		push(char, i);
		i++;
	}
	return { plain, map, links };
};

const collapse = (value: Projected): Projected => {
	let plain = '';
	const map: number[] = [];
	let pendingSpace = false;
	for (let i = 0; i < value.plain.length; i++) {
		const char = value.plain[i];
		if (/\s/.test(char)) {
			if (plain) pendingSpace = true;
			continue;
		}
		if (pendingSpace) {
			plain += ' ';
			map.push(value.map[i]);
			pendingSpace = false;
		}
		plain += char;
		map.push(value.map[i]);
	}
	return { plain, map };
};

export interface SelectionMatch {
	text: string;
	start: number;
	end: number;
}

export const findSelectionInSource = (
	body: string,
	selected: string,
): SelectionMatch | null => {
	const wanted = selected.replace(/\s+/g, ' ').trim();
	if (!wanted) return null;

	const projected = project(body);
	const collapsed = collapse(projected);
	const at = collapsed.plain.indexOf(wanted);
	if (at < 0) return null;
	let start = collapsed.map[at];
	let end = collapsed.map[at + wanted.length - 1] + 1;

	for (const link of projected.links) {
		if (
			start >= link.textStart &&
			start < link.textEnd &&
			end >= link.textEnd
		) {
			if (start === link.textStart) start = link.open;
		}
		if (
			end > link.textStart &&
			end <= link.textEnd &&
			start <= link.textStart
		) {
			if (end === link.textEnd) end = link.close;
		}
	}
	for (const marker of MARKERS) {
		const inner = body.slice(start, end).split(marker).length - 1;
		const before = body.slice(start - marker.length, start) === marker;
		const after = body.startsWith(marker, end);
		if (before && after && inner % 2 === 0) {
			start -= marker.length;
			end += marker.length;
			break;
		}
		if (before && inner % 2 === 1) start -= marker.length;
		if (after && inner % 2 === 1) end += marker.length;
	}

	const text = body.slice(start, end);
	if (!text.trim()) return null;
	return { text, start, end };
};

export interface ActionLine {
	text: string;
	line: number;
}

const HEADING_LINE = /^(#{1,6})\s+(.+?)\s*#*\s*$/;
const ITEM = /^[ \t]*(?:[-*+]|\d+[.)])[ \t]+(?:\[[ xX]\][ \t]+)?(.+?)\s*$/;
const TASK_LINK = /\[[^\]]*\]\(tmgr:\/\/task\/\d+\)/g;

export const meetingActionLines = (
	body: string,
	heading = 'Действия',
): ActionLine[] => {
	const lines = body.split(/\r?\n/);
	const result: ActionLine[] = [];
	let level = 0;
	let inFence = false;
	lines.forEach((line, index) => {
		if (/^ {0,3}(```|~~~)/.test(line)) inFence = !inFence;
		if (inFence) return;
		const found = HEADING_LINE.exec(line);
		if (found) {
			if (level) {
				if (found[1].length <= level) level = 0;
				else return;
			}
			if (!level && found[2].trim().toLowerCase() === heading.toLowerCase()) {
				level = found[1].length;
			}
			return;
		}
		if (!level) return;
		const item = ITEM.exec(line);
		if (!item) return;
		const text = item[1];
		if (!text.replace(TASK_LINK, '').replace(/[\s\-–—·:.,;]/g, '')) return;
		result.push({ text, line: index });
	});
	return result;
};
