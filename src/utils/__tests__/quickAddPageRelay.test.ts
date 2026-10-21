import { handlePageAppended } from '../quickAddPageRelay';

const invalidate = jest.fn();
jest.mock('@/actions/tmgr/pages', () => ({
	invalidatePages: () => invalidate(),
}));

const payload = {
	workspace_code: 'work',
	page: { id: 7, slug: 'ivan', title: 'Ivan', version: 5 },
};

const setup = (code: string | undefined) => {
	const commit = jest.fn();
	return {
		commit,
		store: { getters: { currentWorkspace: code ? { code } : null }, commit },
	};
};

beforeEach(() => invalidate.mockReset());

describe('handlePageAppended', () => {
	it('invalidates pages and raises a page.updated event in the current workspace', () => {
		const { store, commit } = setup('work');
		handlePageAppended(store, payload);
		expect(invalidate).toHaveBeenCalled();
		expect(commit).toHaveBeenCalledWith('pagesEvent', {
			type: 'page.updated',
			page: payload.page,
		});
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

describe('handleQuickAddCreated', () => {
	const { tourBus } = jest.requireActual('@/tours/events');
	const { handleQuickAddCreated } = jest.requireActual('../quickAddPageRelay');

	it.each(['task', 'routine'])(
		'maps a %s to task.created from quick add',
		(kind) => {
			const seen: unknown[] = [];
			const off = tourBus.on('task.created', (p: unknown) => seen.push(p));
			handleQuickAddCreated({ kind });
			off();
			expect(seen).toEqual([{ source: 'quick_add', kind }]);
		},
	);

	it('drops routine snapshots only for a routine', () => {
		const { requestCache } = jest.requireActual('@/utils/requestCache');
		requestCache.set('routines-snapshot{}', { entries: [] });
		handleQuickAddCreated({ kind: 'task' });
		expect(requestCache.get('routines-snapshot{}')).toEqual({ entries: [] });
		handleQuickAddCreated({ kind: 'routine' });
		expect(requestCache.get('routines-snapshot{}')).toBeNull();
	});

	it('ignores an unknown or missing kind', () => {
		const seen: unknown[] = [];
		const off = tourBus.on('task.created', (p: unknown) => seen.push(p));
		handleQuickAddCreated({ kind: 'page' });
		handleQuickAddCreated(null);
		off();
		expect(seen).toEqual([]);
	});
});
