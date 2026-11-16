import { lineNumber, qualifyFile, sideOf, threadKey } from './diff';
import type { DiffFile, DiffGroup, DiffLine } from './types';

export type DiffRow = { id: string } & (
	| { kind: 'group'; group: DiffGroup }
	| {
			kind: 'note';
			note: 'error' | 'none' | 'truncated';
			text?: string;
	  }
	| {
			kind: 'file';
			file: DiffFile;
			bottom: boolean;
	  }
	| { kind: 'hunk'; header: string; bottom: boolean }
	| {
			kind: 'line';
			qpath: string;
			line: DiffLine;
			key: string;
			bottom: boolean;
	  }
	| {
			kind: 'thread';
			qpath: string;
			line: DiffLine;
			key: string;
			bottom: boolean;
	  }
);

export interface DiffRows {
	rows: DiffRow[];
	threadIndex: Map<string, number>;
	lineIndex: Map<string, number>;
	maxChars: number;
}

export const buildDiffRows = (
	groups: DiffGroup[],
	active: ReadonlySet<string>,
): DiffRows => {
	const rows: DiffRow[] = [];
	const threadIndex = new Map<string, number>();
	const lineIndex = new Map<string, number>();
	let maxChars = 0;
	const multi = groups.length > 1;

	groups.forEach((group, gi) => {
		if (multi) rows.push({ id: `g${gi}`, kind: 'group', group });
		const { view } = group;
		if (view.error) {
			rows.push({
				id: `n${gi}e`,
				kind: 'note',
				note: 'error',
				text: view.error,
			});
		} else if (view.files.length === 0) {
			rows.push({ id: `n${gi}n`, kind: 'note', note: 'none' });
		}
		if (view.truncated) {
			rows.push({ id: `n${gi}t`, kind: 'note', note: 'truncated' });
		}

		view.files.forEach((file, fi) => {
			const qpath = qualifyFile(group.label, file.path, groups.length);
			rows.push({ id: `f${gi}.${fi}`, kind: 'file', file, bottom: false });
			file.hunks.forEach((hunk, hi) => {
				rows.push({
					id: `h${gi}.${fi}.${hi}`,
					kind: 'hunk',
					header: hunk.header,
					bottom: false,
				});
				if (hunk.header.length > maxChars) maxChars = hunk.header.length;
				hunk.lines.forEach((line, li) => {
					const id = `l${gi}.${fi}.${hi}.${li}`;
					if (line.text.length + 1 > maxChars) maxChars = line.text.length + 1;
					const number = lineNumber(line);
					if (number === null) {
						rows.push({
							id,
							kind: 'line',
							qpath,
							line,
							key: '',
							bottom: false,
						});
						return;
					}
					const key = threadKey(qpath, number, sideOf(line));
					rows.push({ id, kind: 'line', qpath, line, key, bottom: false });
					if (!lineIndex.has(key)) {
						lineIndex.set(key, rows.length - 1);
						if (active.has(key)) {
							rows.push({
								id: `t${key}`,
								kind: 'thread',
								qpath,
								line,
								key,
								bottom: false,
							});
							threadIndex.set(key, rows.length - 1);
						}
					}
				});
			});
			const last = rows[rows.length - 1];
			if (last.kind !== 'group' && last.kind !== 'note') last.bottom = true;
		});
	});
	return { rows, threadIndex, lineIndex, maxChars };
};
