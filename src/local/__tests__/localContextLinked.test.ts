import { localContext } from '../runtime';

const mockStore = { state: { localProfile: null as unknown } };

jest.mock('@/store', () => ({
	__esModule: true,
	get default() {
		return mockStore;
	},
}));
jest.mock('../schema', () => ({ migrate: jest.fn().mockResolvedValue(0) }));
jest.mock('../pages/service', () => ({
	ensureContextPage: jest.fn().mockResolvedValue(undefined),
}));
jest.mock('@tauri-apps/api/core', () => ({
	invoke: jest.fn().mockResolvedValue([]),
	convertFileSrc: (path: string) => path,
}));

const workspace = {
	code: 'w',
	name: 'W',
	id: -1,
	schema_version: 1,
	created_at: '2026-01-01T00:00:00Z',
} as any;
const user = (id: number) => ({ id, name: 'U', email: '' }) as any;

describe('localContext localOwnerLinked', () => {
	it('is set for the linked account from the stored profile', async () => {
		mockStore.state.localProfile = { linked_user_id: 7 };
		const ctx = await localContext(workspace, user(7));
		expect(ctx.localOwnerLinked).toBe(true);
	});

	it('is not set for another account', async () => {
		mockStore.state.localProfile = { linked_user_id: 7 };
		const ctx = await localContext(workspace, user(8));
		expect(ctx.localOwnerLinked).toBeUndefined();
	});

	it('is not set when the claim is missing from the profile', async () => {
		mockStore.state.localProfile = { id: -1 };
		const ctx = await localContext(workspace, user(7));
		expect(ctx.localOwnerLinked).toBeUndefined();
	});

	it('is not set in local mode', async () => {
		mockStore.state.localProfile = { linked_user_id: 7 };
		const ctx = await localContext(workspace, user(-1));
		expect(ctx.localOwnerLinked).toBeUndefined();
	});
});
