import { loadTaskFormReferenceData } from '../taskFormReferenceData';

const makeDeps = () => ({
	getStatuses: jest.fn().mockResolvedValue([{ id: 1 }]),
	getStatusesOfWorkspace: jest.fn(async (id: number) => [{ id: id * 10 }]),
	getCategories: jest.fn().mockResolvedValue([{ id: 2 }]),
	getCategoriesOfWorkspace: jest.fn(async (id: number) => [{ id: id * 20 }]),
	getWorkspaceMembers: jest.fn(async (id: number) => [{ id: id * 30 }]),
	getAssignablePersonas: jest.fn(async (id: number) => [{ id: id * 40 }]),
});

describe('loadTaskFormReferenceData', () => {
	it('loads everything for the task workspace when it differs from the current one', async () => {
		const deps = makeDeps();
		const result = await loadTaskFormReferenceData(
			{ taskWorkspaceId: 106, currentWorkspaceId: 56 },
			deps as any,
		);
		expect(result.statuses).toEqual([{ id: 1060 }]);
		expect(result.categories).toEqual([{ id: 2120 }]);
		expect(result.members).toEqual([{ id: 3180 }]);
		expect(result.personas).toEqual([{ id: 4240 }]);
		expect(deps.getStatuses).not.toHaveBeenCalled();
		expect(deps.getCategories).not.toHaveBeenCalled();
		expect(deps.getWorkspaceMembers).not.toHaveBeenCalledWith(56);
	});

	it('uses the current workspace endpoints for a new task', async () => {
		const deps = makeDeps();
		const result = await loadTaskFormReferenceData(
			{ taskWorkspaceId: null, currentWorkspaceId: 56 },
			deps as any,
		);
		expect(result.statuses).toEqual([{ id: 1 }]);
		expect(result.categories).toEqual([{ id: 2 }]);
		expect(result.members).toEqual([{ id: 1680 }]);
		expect(deps.getStatusesOfWorkspace).not.toHaveBeenCalled();
	});

	it('skips members and personas when no workspace is known', async () => {
		const deps = makeDeps();
		const result = await loadTaskFormReferenceData(
			{ taskWorkspaceId: null, currentWorkspaceId: null },
			deps as any,
		);
		expect(result.members).toEqual([]);
		expect(result.personas).toEqual([]);
	});

	it('tolerates a failing personas request', async () => {
		const deps = makeDeps();
		deps.getAssignablePersonas.mockRejectedValue(new Error('x'));
		const result = await loadTaskFormReferenceData(
			{ taskWorkspaceId: 106, currentWorkspaceId: 56 },
			deps as any,
		);
		expect(result.personas).toEqual([]);
	});
});
