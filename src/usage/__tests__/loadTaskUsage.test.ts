import { usageScan } from '../api';
import { collectRuns, resetRunsCache, scanThenRead } from '../loadTaskUsage';
import { fetchRuns } from '../runs';
import { pricedCost } from '../taskModel';
import { usageTask } from '../taskUsage';

jest.mock('../runs', () => ({
	fetchRuns: jest.fn(),
	toRawRun: (item: any, workspace: string) => ({ ...item, workspace }),
}));
jest.mock('../taskUsage', () => ({ usageTask: jest.fn() }));
jest.mock('../api', () => ({ usageScan: jest.fn() }));

const fetchMock = fetchRuns as jest.Mock;

describe('collectRuns', () => {
	beforeEach(() => {
		resetRunsCache();
		fetchMock.mockReset();
	});

	it('merges the task own runs into the workspace runs without duplicates', async () => {
		fetchMock.mockResolvedValue({
			runs: [{ id: 1 }, { id: 2 }],
			failedWorkspaces: 0,
		});
		const runs = await collectRuns({
			taskId: 5,
			workspaceId: 3,
			workspaceCode: 'ws',
			ownRuns: [{ id: 2 }, { id: 9 }],
		});
		expect(runs.map((r) => r.id).sort()).toEqual([1, 2, 9]);
	});

	it('falls back to the own runs when the workspace fetch fails', async () => {
		fetchMock.mockResolvedValue({ runs: [], failedWorkspaces: 1 });
		const runs = await collectRuns({
			taskId: 5,
			workspaceId: 3,
			workspaceCode: 'ws',
			ownRuns: [{ id: 9 }],
		});
		expect(runs.map((r) => r.id)).toEqual([9]);
	});

	it('does not fetch for local workspaces or an unknown code', async () => {
		await collectRuns({
			taskId: 5,
			workspaceId: -1,
			workspaceCode: 'l',
			ownRuns: [{ id: 1 }],
		});
		await collectRuns({
			taskId: 5,
			workspaceId: 3,
			workspaceCode: '',
			ownRuns: [{ id: 1 }],
		});
		expect(fetchMock).not.toHaveBeenCalled();
	});

	it('reuses the workspace runs for a minute', async () => {
		fetchMock.mockResolvedValue({ runs: [{ id: 1 }], failedWorkspaces: 0 });
		const req = { taskId: 5, workspaceId: 3, workspaceCode: 'ws', ownRuns: [] };
		await collectRuns(req, 1000);
		await collectRuns(req, 30_000);
		expect(fetchMock).toHaveBeenCalledTimes(1);
	});
});

describe('scanThenRead', () => {
	it('reads nothing when log reading is off on this device', async () => {
		(usageScan as jest.Mock).mockResolvedValue({
			state: 'off',
			files: 0,
			events_new: 0,
			events_updated: 0,
			errors: 0,
			bad_lines: 0,
		});
		const req = { taskId: 5, workspaceCode: 'ws', ownRuns: [] };
		expect(await scanThenRead(req, [])).toBeNull();
		expect(usageScan).toHaveBeenCalledTimes(1);
		expect(usageTask).not.toHaveBeenCalled();
	});
});

describe('pricedCost', () => {
	const totals = (
		cost_usd: number,
		events: number,
		unpriced_events: number,
	) => ({
		totals: { cost_usd, events, unpriced_events },
	});
	it('hides the cost when every event is unpriced', () => {
		expect(pricedCost(totals(0, 4, 4))).toBeNull();
	});
	it('keeps a priced cost and null input', () => {
		expect(pricedCost(totals(1.5, 4, 1))).toBe(1.5);
		expect(pricedCost(null)).toBeNull();
	});
});
