import type { Page, UpdatePagePayload } from '@/actions/tmgr/pages';

export interface PageDraft {
	title: string;
	body: string;
	properties: Record<string, any>;
}

export type ConflictChoice = 'theirs' | 'mine' | 'both';

export type ConflictOutcome =
	| {
			kind: 'adopt';
			page: Pick<Page, 'id' | 'version' | 'title' | 'body' | 'properties'>;
	  }
	| { kind: 'retry'; payload: UpdatePagePayload }
	| { kind: 'compare' };

export const changedFields = (
	draft: PageDraft,
	base: PageDraft,
): Omit<UpdatePagePayload, 'version'> => {
	const fields: Omit<UpdatePagePayload, 'version'> = {};
	if (draft.title !== base.title) fields.title = draft.title;
	if (draft.body !== base.body) fields.body = draft.body;
	if (JSON.stringify(draft.properties) !== JSON.stringify(base.properties)) {
		fields.properties = draft.properties;
	}
	return fields;
};

export const resolveConflict = (
	choice: ConflictChoice,
	state: {
		draft: PageDraft;
		base: PageDraft;
		theirs: Pick<Page, 'id' | 'version' | 'title' | 'body' | 'properties'>;
	},
): ConflictOutcome => {
	if (choice === 'both') return { kind: 'compare' };
	if (choice === 'theirs') return { kind: 'adopt', page: state.theirs };
	const fields = changedFields(state.draft, state.base);
	if (!Object.keys(fields).length) return { kind: 'adopt', page: state.theirs };
	return {
		kind: 'retry',
		payload: { version: state.theirs.version, ...fields },
	};
};
