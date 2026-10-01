import { handlePageAppended } from '../quickAddPageRelay';

const invalidate = jest.fn();
jest.mock('@/actions/tmgr/pages', () => ({ invalidatePages: () => invalidate() }));

const payload = {
	workspace_code: 'work',
	page: { id: 7, slug: 'ivan', title: 'Ivan', version: 5 },
};

const setup = (code: string | undefined) => {
	const commit = jest.fn();
	return { commit, store: { getters: { currentWorkspace: code ? { code } : null }, commit } };
};

beforeEach(() => invalidate.mockReset());

describe('handlePageAppended', () => {
	it('invalidates pages and raises a page.updated event in the current workspace', () => {
		const { store, commit } = setup('work');
		handlePageAppended(store, payload);
		expect(invalidate).toHaveBeenCalled();
		expect(commit).toHaveBeenCalledWith('pagesEvent', { type: 'page.updated', page: payload.page });
	});

	it('only invalidates when another workspace is open', () => {
		const { store, commit } = setup('other');
		handlePageAppended(store, payload);
		expect(invalidate).toHaveBeenCalled();
		expect(commit).not.toHaveBeenCalled();
	});

	it('ignores malformed payloads', () => {
		const { store, commit } = setup('work');
		handlePageAppended(store, null);
		handlePageAppended(store, { workspace_code: 'work' } as any);
		expect(invalidate).not.toHaveBeenCalled();
		expect(commit).not.toHaveBeenCalled();
	});
});
