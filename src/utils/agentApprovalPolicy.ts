import { t } from '@/i18n';

import { norm } from '@/utils/agentApproval';

export type ApprovalState = 'none' | 'active' | 'stale' | 'revoked';

export interface TaskText {
	title: string | null | undefined;
	description: string | null | undefined;
}

export const sameText = (a: TaskText, b: TaskText): boolean =>
	norm(a.title) === norm(b.title) &&
	norm(a.description) === norm(b.description);

export const shouldAutoSign = (input: {
	submitted: TaskText;
	saved: TaskText;
	isOwnerSave: boolean;
	authorId: number | null | undefined;
	meId: number | null | undefined;
	state: ApprovalState;
	settingEnabled: boolean;
	ownerAuthored: boolean;
}): boolean => {
	if (!input.settingEnabled || !input.isOwnerSave) return false;
	if (!sameText(input.submitted, input.saved)) return false;
	if (input.state === 'active' || input.state === 'stale') return true;
	return (
		input.state === 'none' &&
		input.ownerAuthored === true &&
		input.authorId != null &&
		input.meId != null &&
		input.authorId === input.meId
	);
};

export interface CategoryRepositories {
	categoryId: string;
	names: string[];
}

export interface AllowedEntry {
	workspaceCode: string;
	workspaceId: number;
	expiresAt: string | null;
	expired: boolean;
	keys: number;
	repos: number;
	repositories?: CategoryRepositories[];
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
			return t('taskAgent.helpers.approval.revokedBy.ui');
		case 'cli':
			return t('taskAgent.helpers.approval.revokedBy.cli');
		case 'mcp':
			return t('taskAgent.helpers.approval.revokedBy.mcp');
		case 'persona':
			return t('taskAgent.helpers.approval.revokedBy.persona');
		case 'plugin':
			return t('taskAgent.helpers.approval.revokedBy.plugin');
		case 'legacy':
			return t('taskAgent.helpers.approval.revokedBy.legacy');
		default:
			return t('taskAgent.helpers.approval.revokedBy.other');
	}
};

export const revokedReasonLabel = (reason: string | null | undefined) => {
	switch (reason) {
		case 'title_changed':
		case 'description_changed':
		case 'content_changed':
			return t('taskAgent.helpers.approval.reasonEdited');
		case 'comment':
		case 'comment_added':
			return t('taskAgent.helpers.approval.reasonComment');
		default:
			return reason
				? reason.replace(/_/g, ' ')
				: t('taskAgent.helpers.approval.reasonChanged');
	}
};

export interface CategoryFields {
	project_category_id?: number | null;
	category?: number | { id: number; code?: string; title?: string } | null;
}

export const categoryIdOf = (task: CategoryFields): number => {
	const fromCategory =
		task.category && typeof task.category === 'object'
			? task.category.id
			: task.category;
	return Number(task.project_category_id ?? fromCategory ?? 0) || 0;
};

export const categoryLabel = (task: CategoryFields): string => {
	const id = categoryIdOf(task);
	if (!id) return t('taskAgent.helpers.approval.noCategory');
	const category = task.category;
	if (category && typeof category === 'object' && category.id === id) {
		const parts = [category.code, category.title].filter(Boolean);
		if (parts.length) return parts.join(' - ');
	}
	return t('taskAgent.helpers.approval.category', { id });
};
