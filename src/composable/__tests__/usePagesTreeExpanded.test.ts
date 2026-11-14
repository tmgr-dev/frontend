import { getPagesTree } from '@/actions/tmgr/pages';
import { effectScope, ref } from 'vue';
import { usePagesTree } from '../usePagesTree';

jest.mock('@/actions/tmgr/pages', () => ({
	getPagesTree: jest.fn(),
	movePage: jest.fn(),
}));
jest.mock('../usePagesRealtime', () => ({ usePagesRealtime: jest.fn() }));

describe('usePagesTree expanded state', () => {
	const store: Record<string, string> = {};
	beforeEach(() => {
		Object.keys(store).forEach((k) => delete store[k]);
		(global as any).localStorage = {
			getItem: (k: string) => (k in store ? store[k] : null),
			setItem: (k: string, v: string) => {
				store[k] = v;
			},
		};
		(getPagesTree as jest.Mock).mockResolvedValue([]);
	});

	afterEach(() => {
		delete (global as any).localStorage;
	});

	it('expandMany adds ids once and persists them for a local workspace', () => {
		const tree = usePagesTree(ref(-7), ref(true), () => {});
		tree.expandMany([-3, -2]);
		tree.expandMany([-2]);
		expect(Array.from(tree.expanded.value).sort((a, b) => a - b)).toEqual([
			-3, -2,
		]);
		expect(JSON.parse(store['pages-expanded--7'])).toEqual([-3, -2]);
	});

	it('two trees of one workspace see each other toggles', () => {
		const scope = effectScope();
		const [sidebar, index] = scope.run(() => [
			usePagesTree(ref(5), ref(true), () => {}),
			usePagesTree(ref(5), ref(true), () => {}),
		]) as [ReturnType<typeof usePagesTree>, ReturnType<typeof usePagesTree>];
		index.toggle(10);
		expect(sidebar.expanded.value.has(10)).toBe(true);
		sidebar.toggle(10);
		expect(index.expanded.value.has(10)).toBe(false);
		scope.stop();
	});

	it('ignores changes of another workspace', () => {
		const scope = effectScope();
		const [a, b] = scope.run(() => [
			usePagesTree(ref(5), ref(true), () => {}),
			usePagesTree(ref(6), ref(true), () => {}),
		]) as [ReturnType<typeof usePagesTree>, ReturnType<typeof usePagesTree>];
		a.toggle(1);
		expect(b.expanded.value.has(1)).toBe(false);
		scope.stop();
	});
});
