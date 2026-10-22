export interface QuickAddDraftFields {
	mode: 'task' | 'page';
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
