jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn() },
}));
jest.mock('@/store', () => ({
	__esModule: true,
	default: {
		state: { user: { id: 1 }, token: 'token1' },
		getters: { currentWorkspaceId: 1 },
		commit: jest.fn(),
	},
}));
import axios from '@/plugins/axios';
import store from '@/store';
import { requestCache } from '@/utils/requestCache';
import { getCategories } from '../categories';
import { getStatuses } from '../statuses';
import { getWorkspaceMembers, getWorkspaceStatuses } from '../workspaces';
const deferred = () => {
	let resolve!: (v: any) => void;
	const promise = new Promise<any>((a) => {
		resolve = a;
	});
	return { promise, resolve };
};
beforeEach(() => {
	requestCache.clear();
	jest.clearAllMocks();
});
test.each([
	['categories', () => getCategories()],
	['statuses', () => getStatuses()],
	['workspace-1-members', () => getWorkspaceMembers(1)],
])('%s pending result cannot restore cache after clear', async (key, load) => {
	const old = deferred();
	(axios.get as jest.Mock).mockReturnValue(old.promise);
	const pending = (load as () => Promise<unknown>)();
	requestCache.clear();
	old.resolve({ data: { data: [{ id: 1 }] } });
	await pending;
	expect(requestCache.has(key as string)).toBe(false);
});
test('workspace statuses cannot commit across a context change', async () => {
	const old = deferred();
	(axios.get as jest.Mock).mockReturnValue(old.promise);
	const pending = getWorkspaceStatuses();
	(store.getters as any).currentWorkspaceId = 2;
	requestCache.clear();
	old.resolve({ data: { data: [{ id: 1 }] } });
	await pending;
	expect(store.commit).not.toHaveBeenCalled();
});
