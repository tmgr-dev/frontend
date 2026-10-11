import { installNavHistory, useNavHistory } from '../useNavHistory';

const makeEnv = () => {
	const hooks: Array<(to: { path: string }) => void> = [];
	const watchers: Array<() => void> = [];
	const state = { currentTaskIdForModal: null as any };
	const route = { value: { fullPath: '/ws/list' } };
	const router = {
		currentRoute: route,
		afterEach: (h: (to: { path: string }) => void) => hooks.push(h),
		push: async (path: string) => {
			route.value = { fullPath: path };
			hooks.forEach((h) => h({ path }));
		},
	};
	const store = {
		state,
		watch: (_g: () => unknown, cb: () => void) => watchers.push(cb),
		commit: (type: string, id?: any) => {
			state.currentTaskIdForModal = type === 'closeTaskModal' ? null : id;
			watchers.forEach((w) => w());
		},
	};
	return { router, store, state, route };
};

describe('nav history glue', () => {
	it('walks A -> B -> C back and forward, across routes and modal tasks', async () => {
		const { router, store, state, route } = makeEnv();
		installNavHistory(router, store);
		const nav = useNavHistory();
		expect(nav.canBack.value).toBe(false);

		store.commit('setCurrentTaskIdForModal', 1);
		store.commit('setCurrentTaskIdForModal', 2);
		await router.push('/ws/board');
		expect(nav.canBack.value).toBe(true);

		await nav.back();
		expect(route.value.fullPath).toBe('/ws/list');
		expect(state.currentTaskIdForModal).toBe(2);
		await nav.back();
		expect(state.currentTaskIdForModal).toBe(1);
		await nav.back();
		expect(state.currentTaskIdForModal).toBeNull();
		expect(nav.canBack.value).toBe(false);
		expect(nav.canForward.value).toBe(true);

		await nav.forward();
		expect(state.currentTaskIdForModal).toBe(1);
		await nav.forward();
		await nav.forward();
		expect(route.value.fullPath).toBe('/ws/board');
		expect(nav.canForward.value).toBe(false);
	});
});
