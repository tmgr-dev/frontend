export type QuickAddType = 'routine' | 'task' | 'screenshot' | 'page';

export const DEFAULT_TYPE: QuickAddType = 'routine';

export interface QuickAddDraftFields {
	mode: QuickAddType;
	title: string;
	note: string;
	pageText: string;
}

export interface QuickAddOpenPayload {
	text?: string | null;
	screenshot?: string | null;
}

export interface QuickAddOpenPlan {
	keep: boolean;
	text: 'set' | 'append' | null;
	screenshot: 'attach' | 'offer' | null;
}

export const hasDraft = (
	fields: QuickAddDraftFields,
	hasScreenshot: boolean,
): boolean =>
	hasScreenshot ||
	Boolean(
		fields.mode === 'page'
			? fields.pageText.trim()
			: fields.title.trim() || fields.note.trim(),
	);

/** A reopen never overwrites an unfinished draft: new text is appended, a second screenshot is only offered. */
export const planOpen = (
	draft: { hasDraft: boolean; hasScreenshot: boolean },
	payload: QuickAddOpenPayload,
): QuickAddOpenPlan => {
	const text = payload.text?.trim()
		? draft.hasDraft
			? 'append'
			: 'set'
		: null;
	const screenshot = payload.screenshot
		? draft.hasDraft && draft.hasScreenshot
			? 'offer'
			: 'attach'
		: null;
	return { keep: draft.hasDraft, text, screenshot };
};

export const appendText = (current: string, extra: string): string =>
	[current.trimEnd(), extra.trim()].filter(Boolean).join('\n\n');

/** A screenshot turns a routine or task draft into a task with a screenshot; a page draft stays a page. */
export const typeAfterScreenshot = (type: QuickAddType): QuickAddType =>
	type === 'page' ? 'page' : 'screenshot';

export const isTaskType = (type: QuickAddType): boolean =>
	type === 'task' || type === 'screenshot';

export type SubmitKind = 'routine' | 'task' | 'task-screenshot' | 'page';

/** The chosen type alone decides what is created; a kept screenshot never changes it. */
export const submitKind = (type: QuickAddType): SubmitKind =>
	type === 'screenshot' ? 'task-screenshot' : type;

export const canSubmitDraft = (
	type: QuickAddType,
	fields: { title: string; pageText: string; pageId: number | null },
	hasScreenshot: boolean,
): boolean => {
	if (type === 'page') {
		return Boolean(fields.pageId && (fields.pageText.trim() || hasScreenshot));
	}
	if (!fields.title.trim()) return false;
	return type === 'screenshot' ? hasScreenshot : true;
};

/** A screenshot that stays in the draft but is not sent by the current type. */
export const isScreenshotUnsent = (
	type: QuickAddType,
	hasScreenshot: boolean,
): boolean => hasScreenshot && (type === 'routine' || type === 'task');

const CATEGORY_KEY = 'desktop.quickAdd.category.';
const NO_CATEGORY = 'none';

/** `undefined` means nothing is remembered yet; `null` is a remembered "No category". */
export const readLastCategory = (
	workspaceId: number,
): number | null | undefined => {
	try {
		const raw = localStorage.getItem(CATEGORY_KEY + workspaceId);
		if (raw === NO_CATEGORY) return null;
		return Number(raw) || undefined;
	} catch {
		return undefined;
	}
};

export const writeLastCategory = (
	workspaceId: number,
	categoryId: number | null,
): void => {
	try {
		localStorage.setItem(
			CATEGORY_KEY + workspaceId,
			categoryId === null ? NO_CATEGORY : String(categoryId),
		);
	} catch {
		/* storage unavailable: the category is not remembered */
	}
};

export const pickRememberedCategory = (
	categories: { id: number }[],
	remembered: number | null | undefined,
): number | null =>
	remembered != null && categories.some((c) => c.id === remembered)
		? remembered
		: null;
