import { getUserHeatmap } from '@/actions/tmgr/dashboard';
import type { HeatmapData } from '@/types/dashboard';
import { requestCache } from '@/utils/requestCache';
import { effectScope, nextTick, ref } from 'vue';
import { readStoredRange, useHeatmapRange } from '../useHeatmapRange';

jest.mock('@/actions/tmgr/dashboard', () => ({ getUserHeatmap: jest.fn() }));
const fetchHeatmap = getUserHeatmap as jest.Mock;

const KEY = 'tmgr:dashboard:heatmap-range:7:1';
const heatmap = (extra: Partial<HeatmapData> = {}): HeatmapData => ({
	contributions: [],
	total_contributions: 0,
	streak: { current: 0, longest: 0 },
	year: 2026,
	weeks: 53,
	...extra,
});
const yearResponse = (year: number) =>
	heatmap({ year, from: `${year}-01-01`, to: `${year}-12-31` });
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

let storage: Map<string, string>;
const setup = (data: HeatmapData | null, workspace = ref(1)) => {
	const heatmapData = ref<HeatmapData | null>(data);
	const scope = effectScope();
	const range = scope.run(() =>
		useHeatmapRange({
			heatmapData,
			workspaceId: () => workspace.value,
			userId: () => 7,
		}),
	)!;
	return { range, heatmapData, scope, workspace };
};

describe('useHeatmapRange (TM-701)', () => {
	beforeEach(() => {
		jest.resetAllMocks();
		requestCache.clear();
		storage = new Map();
		(globalThis as any).localStorage = {
			getItem: (k: string) => storage.get(k) ?? null,
			setItem: (k: string, v: string) => storage.set(k, v),
		};
		fetchHeatmap.mockImplementation(async (_ws, params) => ({
			success: true,
			data: yearResponse(params.year),
		}));
	});
	afterAll(() => {
		delete (globalThis as any).localStorage;
	});

	it('loads a selected year without touching the default heatmapData', async () => {
		const defaults = heatmap({ years: [2026, 2025] });
		const { range, heatmapData, scope } = setup(defaults);
		const original = heatmapData.value;
		expect(range.data.value).toBe(original);
		expect(fetchHeatmap).not.toHaveBeenCalled();

		range.select('2025');
		await nextTick();
		await flush();
		expect(fetchHeatmap).toHaveBeenCalledWith(1, { range: 'year', year: 2025 });
		expect(range.data.value?.year).toBe(2025);
		expect(heatmapData.value).toBe(original);
		expect(heatmapData.value?.year).toBe(2026);
		expect(storage.get(KEY)).toBe('2025');

		range.select('last_12_months');
		await nextTick();
		expect(range.data.value).toBe(original);
		expect(fetchHeatmap).toHaveBeenCalledTimes(1);
		scope.stop();
	});

	it('ignores a stored year that is not in `years`, and any year without `years`', async () => {
		storage.set(KEY, '2019');
		const a = setup(heatmap({ years: [2026, 2025] }));
		expect(a.range.selectValue.value).toBe('last_12_months');
		a.scope.stop();

		storage.set(KEY, '2025');
		const b = setup(heatmap());
		expect(b.range.years.value).toBeNull();
		expect(b.range.year.value).toBeNull();
		await flush();
		expect(fetchHeatmap).not.toHaveBeenCalled();
		b.scope.stop();
	});

	it('survives a throwing localStorage', () => {
		(globalThis as any).localStorage = {
			getItem: () => {
				throw new Error('denied');
			},
			setItem: () => {
				throw new Error('denied');
			},
		};
		expect(readStoredRange(KEY)).toBe('last_12_months');
		const { range, scope } = setup(heatmap({ years: [2026, 2025] }));
		expect(() => range.select('2025')).not.toThrow();
		expect(range.year.value).toBe(2025);
		scope.stop();
	});

	it('drops a late response after a fast switch or a workspace change', async () => {
		const resolvers: Array<() => void> = [];
		fetchHeatmap.mockImplementation(
			(_ws, params) =>
				new Promise((resolve) =>
					resolvers.push(() =>
						resolve({ success: true, data: yearResponse(params.year) }),
					),
				),
		);
		const { range, scope, workspace } = setup(
			heatmap({ years: [2026, 2025, 2024] }),
		);
		range.select('2025');
		await nextTick();
		range.select('2024');
		await nextTick();
		resolvers[1]();
		await flush();
		resolvers[0]();
		await flush();
		expect(range.data.value?.year).toBe(2024);

		storage.set('tmgr:dashboard:heatmap-range:7:2', '2024');
		range.select('2025');
		await nextTick();
		workspace.value = 2;
		await nextTick();
		expect(fetchHeatmap).toHaveBeenLastCalledWith(2, {
			range: 'year',
			year: 2024,
		});
		resolvers[2]();
		await flush();
		expect(range.data.value).toBeNull();
		expect(range.loading.value).toBe(true);
		resolvers[3]();
		await flush();
		expect(range.data.value?.year).toBe(2024);
		scope.stop();
	});

	it('seeds a year from its own snapshot key and shows an error on failure', async () => {
		const defaults = heatmap({ years: [2026, 2025] });
		const first = setup(defaults);
		first.range.select('2025');
		await nextTick();
		await flush();
		first.scope.stop();

		fetchHeatmap.mockResolvedValue({ success: false, error: { message: 'x' } });
		const second = setup(defaults);
		expect(second.range.data.value?.year).toBe(2025);
		await flush();
		expect(second.range.error.value).toBe(true);
		expect(second.range.data.value?.year).toBe(2025);
		second.scope.stop();

		requestCache.clear();
		const third = setup(defaults);
		expect(third.range.data.value).toBeNull();
		await flush();
		expect(third.range.error.value).toBe(true);
		third.scope.stop();
	});

	it('falls back to the default view when a year response has no from/to', async () => {
		fetchHeatmap.mockResolvedValue({ success: true, data: heatmap() });
		const defaults = heatmap({ years: [2026, 2025] });
		const { range, heatmapData, scope } = setup(defaults);
		range.select('2025');
		await nextTick();
		await flush();
		expect(range.year.value).toBeNull();
		expect(range.data.value).toBe(heatmapData.value);
		expect(storage.get(KEY)).toBe('last_12_months');
		scope.stop();
	});
});
