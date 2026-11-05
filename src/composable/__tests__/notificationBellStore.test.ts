jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn(), put: jest.fn() },
}));
jest.mock('@/composable/usePusher', () => ({
	disconnectRealtime: jest.fn(),
	usePusher: jest.fn(),
}));
jest.mock('@/store/plugins/ntfy-client', () => ({
	__esModule: true,
	default: {},
}));

test('the store has the user in state but no user/getUser getter', () => {
	const memory: Record<string, string> = {};
	(global as any).localStorage = {
		getItem: (k: string) => memory[k] ?? null,
		setItem: (k: string, v: string) => (memory[k] = v),
		removeItem: (k: string) => delete memory[k],
	};
	const store = require('@/store').default;
	store.commit('setUser', { id: 7 });
	expect(store.state.user.id).toBe(7);
	expect(store.getters['user/getUser']).toBeUndefined();
});
