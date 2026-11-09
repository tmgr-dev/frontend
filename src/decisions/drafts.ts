export type DraftField = 'chat' | 'free' | 'note' | 'noteFor';
export type TextDrafts = Record<string, Partial<Record<DraftField, string>>>;

export const DRAFTS_KEY = 'decisions.textDrafts.v1';

const FIELDS: DraftField[] = ['chat', 'free', 'note', 'noteFor'];

export const loadDrafts = (storage: Pick<Storage, 'getItem'>): TextDrafts => {
	try {
		const parsed: unknown = JSON.parse(storage.getItem(DRAFTS_KEY) ?? '{}');
		if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
			return {};
		}
		const result: TextDrafts = {};
		for (const [id, entry] of Object.entries(parsed)) {
			if (!entry || typeof entry !== 'object') continue;
			for (const field of FIELDS) {
				const value = (entry as Record<string, unknown>)[field];
				if (typeof value === 'string' && value !== '') {
					(result[id] ??= {})[field] = value;
				}
			}
		}
		return result;
	} catch {
		return {};
	}
};

export const saveDrafts = (
	storage: Pick<Storage, 'setItem'>,
	drafts: TextDrafts,
): void => {
	try {
		storage.setItem(DRAFTS_KEY, JSON.stringify(drafts));
	} catch {
		return;
	}
};

export const setDraft = (
	drafts: TextDrafts,
	id: string,
	field: DraftField,
	value: string,
): TextDrafts => {
	const next: TextDrafts = { ...drafts };
	const entry = { ...next[id] };
	if (value === '') delete entry[field];
	else entry[field] = value;
	if (Object.keys(entry).length === 0) delete next[id];
	else next[id] = entry;
	return next;
};

export const clearCardDrafts = (drafts: TextDrafts, id: string): TextDrafts => {
	if (!(id in drafts)) return drafts;
	const next = { ...drafts };
	delete next[id];
	return next;
};

export const pruneDrafts = (
	drafts: TextDrafts,
	openIds: ReadonlySet<string>,
): TextDrafts => {
	const keep = Object.keys(drafts).filter((id) => openIds.has(id));
	if (keep.length === Object.keys(drafts).length) return drafts;
	const next: TextDrafts = {};
	for (const id of keep) next[id] = drafts[id];
	return next;
};
