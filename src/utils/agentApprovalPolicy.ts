export type ApprovalState = 'none' | 'active' | 'stale' | 'revoked';

export const shouldAutoSign = (input: {
	isOwnerSave: boolean;
	authorId: number | null | undefined;
	meId: number | null | undefined;
	state: ApprovalState;
	settingEnabled: boolean;
	ownerAuthored: boolean;
}): boolean => {
	if (!input.settingEnabled || !input.isOwnerSave) return false;
	if (input.state === 'active' || input.state === 'stale') return true;
	return (
		input.state === 'none' &&
		input.ownerAuthored === true &&
		input.authorId != null &&
		input.meId != null &&
		input.authorId === input.meId
	);
};

export interface AllowedEntry {
	workspaceCode: string;
	workspaceId: number;
	expiresAt: string | null;
	expired: boolean;
	keys: number;
	repos: number;
}

export const EXPIRING_DAYS = 3;

export const expiringSoon = (
	entries: AllowedEntry[],
	now: number,
): AllowedEntry[] =>
	entries.filter((entry) => {
		if (!entry.expiresAt || entry.expired) return false;
		const at = Date.parse(entry.expiresAt);
		if (Number.isNaN(at)) return false;
		return at > now && at - now <= EXPIRING_DAYS * 86_400_000;
	});

const INVISIBLE_RANGES: [number, number][] = [
	[0x00, 0x08],
	[0x0b, 0x0c],
	[0x0e, 0x1f],
	[0x7f, 0x9f],
	[0xad, 0xad],
	[0x34f, 0x34f],
	[0x61c, 0x61c],
	[0x115f, 0x1160],
	[0x17b4, 0x17b5],
	[0x180b, 0x180f],
	[0x200b, 0x200f],
	[0x2028, 0x202e],
	[0x2060, 0x206f],
	[0x3164, 0x3164],
	[0xfe00, 0xfe0f],
	[0xfeff, 0xfeff],
	[0xffa0, 0xffa0],
	[0xfff9, 0xfffb],
	[0xe0000, 0xe0fff],
];

const isInvisible = (code: number): boolean =>
	INVISIBLE_RANGES.some(([lo, hi]) => code >= lo && code <= hi);

export interface TextPiece {
	text: string;
	flagged: boolean;
}

export const hasInvisible = (text: string): boolean =>
	Array.from(text).some((char) => isInvisible(char.codePointAt(0)!));

export const splitInvisible = (text: string): TextPiece[] => {
	const pieces: TextPiece[] = [];
	for (const char of Array.from(text)) {
		const flagged = isInvisible(char.codePointAt(0)!);
		const shown = flagged
			? `<U+${char
					.codePointAt(0)!
					.toString(16)
					.toUpperCase()
					.padStart(4, '0')}>`
			: char;
		const last = pieces[pieces.length - 1];
		if (last && last.flagged === flagged && !flagged) last.text += shown;
		else pieces.push({ text: shown, flagged });
	}
	return pieces;
};

export const revokedByLabel = (kind: string | null | undefined): string => {
	switch (kind) {
		case 'ui':
			return 'the web app';
		case 'cli':
			return 'the CLI';
		case 'mcp':
			return 'MCP';
		case 'persona':
			return 'an agent persona';
		case 'plugin':
			return 'a plugin';
		case 'legacy':
			return 'a legacy token';
		default:
			return 'an integration';
	}
};

export const revokedReasonLabel = (reason: string | null | undefined) => {
	switch (reason) {
		case 'title_changed':
		case 'description_changed':
		case 'content_changed':
			return 'text edited';
		case 'comment':
		case 'comment_added':
			return 'comment added';
		default:
			return reason ? reason.replace(/_/g, ' ') : 'changed';
	}
};
