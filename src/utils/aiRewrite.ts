export interface RewriteSession {
	revisions: string[];
	selected: number;
}

export const composeRewriteText = (base: string, instruction: string) => {
	const ask = instruction.trim();
	if (!ask) return base;
	return [
		'Rewrite the task description below, following the instruction. Return only the rewritten description.',
		'',
		`<description>\n${base}\n</description>`,
		'',
		`<instruction>\n${ask}\n</instruction>`,
	].join('\n');
};

export const createRewriteSession = (original: string): RewriteSession => ({
	revisions: [original],
	selected: 0,
});

export const addRevision = (
	session: RewriteSession,
	text: string,
): RewriteSession => ({
	revisions: [...session.revisions, text],
	selected: session.revisions.length,
});

export const selectRevision = (
	session: RewriteSession,
	index: number,
): RewriteSession =>
	Number.isInteger(index) && index >= 0 && index < session.revisions.length
		? { ...session, selected: index }
		: session;

export const selectedText = (session: RewriteSession) =>
	session.revisions[session.selected];

export const acceptRevision = (session: RewriteSession, current: string) => ({
	next: selectedText(session),
	previous: current,
});

export const createRequestGuard = () => {
	let latest = 0;
	return {
		begin: () => ++latest,
		isCurrent: (token: number) => token === latest,
		cancel: () => {
			latest++;
		},
	};
};

export const rewriteErrorMessage = (error: unknown): string => {
	const response = (error as { response?: { status?: number; data?: any } })
		?.response;
	const status = response?.status;
	if (!status) return 'No connection to the server. Check it and try again.';
	if (status === 403 && response?.data?.error === 'feature_disabled')
		return 'The AI assistant module is turned off for this workspace.';
	if (status === 403)
		return 'You are not allowed to use the AI assistant here.';
	if (status === 422)
		return 'The text is empty, so there is nothing to rewrite.';
	if (status === 413) return 'The text is too long for the AI assistant.';
	if (status === 429) return 'Too many requests. Wait a moment and try again.';
	return 'The AI assistant failed. Please try again.';
};
