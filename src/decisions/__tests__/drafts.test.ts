import {
	DRAFTS_KEY,
	clearCardDrafts,
	loadDrafts,
	pruneDrafts,
	saveDrafts,
	setDraft,
} from '../drafts';

const memory = () => {
	const data = new Map<string, string>();
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
	};
};

describe('text drafts', () => {
	it('keeps separate text per card and field and survives a reload', () => {
		const storage = memory();
		let drafts = setDraft({}, 'a', 'chat', 'hello');
		drafts = setDraft(drafts, 'a', 'free', 'own answer');
		drafts = setDraft(drafts, 'b', 'chat', 'other');
		saveDrafts(storage, drafts);
		expect(loadDrafts(storage)).toEqual({
			a: { chat: 'hello', free: 'own answer' },
			b: { chat: 'other' },
		});
	});

	it('drops a field and an empty card when the text is emptied', () => {
		let drafts = setDraft({}, 'a', 'chat', 'x');
		drafts = setDraft(drafts, 'a', 'chat', '');
		expect(drafts).toEqual({});
	});

	it('clears all drafts of one card', () => {
		const drafts = setDraft(setDraft({}, 'a', 'chat', 'x'), 'b', 'chat', 'y');
		expect(clearCardDrafts(drafts, 'a')).toEqual({ b: { chat: 'y' } });
	});

	it('prunes drafts of cards that are no longer open', () => {
		const drafts = setDraft(setDraft({}, 'a', 'chat', 'x'), 'b', 'note', 'y');
		expect(pruneDrafts(drafts, new Set(['b']))).toEqual({ b: { note: 'y' } });
	});

	it('ignores corrupt storage', () => {
		const storage = memory();
		storage.setItem(DRAFTS_KEY, '{oops');
		expect(loadDrafts(storage)).toEqual({});
		storage.setItem(DRAFTS_KEY, '[1]');
		expect(loadDrafts(storage)).toEqual({});
	});
});
