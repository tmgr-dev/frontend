import type { AnswerDraft } from './answer';
import type { ActionRunState, CardAction, CardView } from './types';

export interface ActionHeader {
	key: string;
	params: Record<string, string>;
}

export interface RunStatus {
	state: ActionRunState;
	url?: string;
	sha?: string;
	error?: string;
}

export const shortSha = (sha: string | undefined): string =>
	(sha ?? '').slice(0, 7);

export const isActionOpen = (card: CardView): boolean =>
	Boolean(card.action) &&
	!card.action?.run &&
	(card.state === 'open' || card.state === 'deferred');

export const fieldValue = (action: CardAction, field: string): string => {
	const value = (action.payload as Record<string, unknown>)[field];
	return typeof value === 'string' ? value : '';
};

export const changedFields = (
	action: CardAction,
	values: Record<string, string>,
): Record<string, string> => {
	const edit: Record<string, string> = {};
	for (const field of action.editable) {
		const value = values[field];
		if (value !== undefined && value !== fieldValue(action, field)) {
			edit[field] = value;
		}
	}
	return edit;
};

export const approveDraft = (
	action: CardAction,
	values: Record<string, string>,
): AnswerDraft => {
	const edit = changedFields(action, values);
	return {
		choice: 'approve',
		action: {
			sha256: action.sha256,
			...(Object.keys(edit).length ? { edit } : {}),
		},
	};
};

export const rejectDraft = (reason: string): AnswerDraft | null => {
	const text = reason.trim();
	return text === '' ? null : { choice: 'reject', note: text };
};

export const actionHeader = (action: CardAction): ActionHeader => {
	const p = action.payload;
	const key = (kind: string) => `decisions.action.header.${kind}`;
	switch (action.kind) {
		case 'comment':
			return {
				key: key('comment'),
				params: { task: `#${p.task_id ?? ''} ${p.task_label ?? ''}`.trim() },
			};
		case 'task':
			return { key: key('task'), params: {} };
		case 'pr':
			return {
				key: key('pr'),
				params: {
					repo: p.repo ?? '',
					branch: p.branch ?? '',
					base: p.base ?? '',
				},
			};
		case 'tag':
			return {
				key: key('tag'),
				params: {
					repo: p.repo ?? '',
					name: p.name ?? '',
					sha: shortSha(p.target_sha),
				},
			};
	}
};

export const runStatus = (action: CardAction): RunStatus | null => {
	const run = action.run;
	if (!run) return null;
	const status: RunStatus = { state: run.state };
	if (run.result?.url) status.url = run.result.url;
	if (run.result?.sha) status.sha = run.result.sha;
	if (run.result?.error) status.error = run.result.error;
	return status;
};
