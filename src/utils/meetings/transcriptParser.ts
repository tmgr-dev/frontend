export interface TranscriptSegment {
	startMs: number | null;
	endMs?: number;
	speaker?: string;
	text: string;
}

export interface ParsedTranscript {
	segments: TranscriptSegment[];
	hasTimecodes: boolean;
}

const TIME = String.raw`(?:\d{1,2}:)?\d{1,2}:\d{2}(?:[.,]\d{1,3})?`;
const CUE = new RegExp(String.raw`^\s*(${TIME})\s*-->\s*(${TIME})`);
const LINE_TIME = new RegExp(
	String.raw`^\s*[\[(]?(${TIME})[\])]?\s*[-–—]?\s*(.*)$`,
);
const SPEAKER = /^([^:\n]{1,40}):\s+(.+)$/;
const VOICE_TAG = /<v\s+([^>]+)>/i;
const TAGS = /<[^>]+>/g;

export const parseTimeToken = (token: string): number => {
	const [clock, fraction = ''] = token.replace(',', '.').split('.');
	const parts = clock.split(':').map(Number);
	const [h, m, s] = parts.length === 3 ? parts : [0, parts[0], parts[1]];
	const ms = fraction ? Number(fraction.padEnd(3, '0').slice(0, 3)) : 0;
	return ((h * 60 + m) * 60 + s) * 1000 + ms;
};

export const formatTimecode = (ms: number): string => {
	const total = Math.max(0, Math.floor(ms / 1000));
	const h = Math.floor(total / 3600);
	const m = Math.floor((total % 3600) / 60);
	const s = total % 60;
	const pad = (n: number) => String(n).padStart(2, '0');
	return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
};

const clean = (text: string): string => text.replace(/\s+/g, ' ').trim();

const parseCues = (lines: string[]): TranscriptSegment[] => {
	const segments: TranscriptSegment[] = [];
	let i = 0;
	while (i < lines.length) {
		const cue = CUE.exec(lines[i]);
		if (!cue) {
			i += 1;
			continue;
		}
		const body: string[] = [];
		i += 1;
		while (i < lines.length && lines[i].trim() !== '') {
			body.push(lines[i]);
			i += 1;
		}
		const raw = body.join(' ');
		const voice = VOICE_TAG.exec(raw);
		const text = clean(raw.replace(TAGS, ''));
		if (!text) continue;
		segments.push({
			startMs: parseTimeToken(cue[1]),
			endMs: parseTimeToken(cue[2]),
			...(voice ? { speaker: voice[1].trim() } : {}),
			text,
		});
	}
	return segments;
};

const parseLines = (lines: string[]): TranscriptSegment[] => {
	const segments: TranscriptSegment[] = [];
	let lead = '';
	for (const line of lines) {
		const match = LINE_TIME.exec(line);
		if (match) {
			let text = clean(match[2]);
			let speaker: string | undefined;
			const named = SPEAKER.exec(text);
			if (named) {
				speaker = named[1].trim();
				text = named[2];
			}
			segments.push({
				startMs: parseTimeToken(match[1]),
				...(speaker ? { speaker } : {}),
				text,
			});
			continue;
		}
		const piece = clean(line);
		if (!piece) continue;
		const last = segments[segments.length - 1];
		if (last) last.text = `${last.text} ${piece}`.trim();
		else lead = `${lead} ${piece}`.trim();
	}
	if (lead && segments.length) segments.unshift({ startMs: 0, text: lead });
	if (lead && !segments.length) segments.push({ startMs: null, text: lead });
	return segments;
};

export const parseTranscript = (input: string): ParsedTranscript => {
	const text = input.replace(/^﻿/, '').replace(/\r\n?/g, '\n');
	if (!text.trim()) return { segments: [], hasTimecodes: false };
	const lines = text.split('\n');
	const cued = lines.some((line) => CUE.test(line));
	const segments = (cued ? parseCues(lines) : parseLines(lines)).filter(
		(segment) => segment.text,
	);
	const hasTimecodes = segments.some((segment) => segment.startMs !== null);
	if (hasTimecodes) {
		segments.sort((a, b) => (a.startMs ?? 0) - (b.startMs ?? 0));
	}
	return { segments, hasTimecodes };
};

export const segmentAt = (
	segments: TranscriptSegment[],
	ms: number,
): TranscriptSegment | undefined => {
	let found: TranscriptSegment | undefined;
	for (const segment of segments) {
		if (segment.startMs === null || segment.startMs > ms) break;
		found = segment;
	}
	return found;
};
