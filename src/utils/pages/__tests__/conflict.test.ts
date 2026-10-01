import { changedFields, resolveConflict, type PageDraft } from '../conflict';

const base: PageDraft = { title: 'T', body: 'base', properties: {} };
const theirs = {
	id: 1,
	version: 7,
	title: 'T2',
	body: 'theirs',
	properties: { a: 1 },
};

describe('changedFields', () => {
	it('lists only differing fields', () => {
		expect(changedFields({ ...base, body: 'mine' }, base)).toEqual({
			body: 'mine',
		});
	});

	it('compares properties structurally', () => {
		expect(
			changedFields(
				{ ...base, properties: { a: 1 } },
				{ ...base, properties: { a: 1 } },
			),
		).toEqual({});
		expect(changedFields({ ...base, properties: { a: 2 } }, base)).toEqual({
			properties: { a: 2 },
		});
	});
});

describe('resolveConflict', () => {
	const mine: PageDraft = { title: 'T', body: 'mine', properties: {} };

	it('theirs adopts the current page', () => {
		expect(resolveConflict('theirs', { draft: mine, base, theirs })).toEqual({
			kind: 'adopt',
			page: theirs,
		});
	});

	it('mine retries with their version and only my changed fields', () => {
		expect(resolveConflict('mine', { draft: mine, base, theirs })).toEqual({
			kind: 'retry',
			payload: { version: 7, body: 'mine' },
		});
	});

	it('mine with no changes degrades to adopting theirs', () => {
		expect(resolveConflict('mine', { draft: base, base, theirs })).toEqual({
			kind: 'adopt',
			page: theirs,
		});
	});

	it('both only shows the comparison', () => {
		expect(resolveConflict('both', { draft: mine, base, theirs })).toEqual({
			kind: 'compare',
		});
	});
});
