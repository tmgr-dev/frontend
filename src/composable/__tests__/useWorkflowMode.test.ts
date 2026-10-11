import { useWorkflowMode } from '../useWorkflowMode';

const mockStore: { getters: Record<string, any>; state: Record<string, any> } =
	{
		getters: {},
		state: {},
	};

jest.mock('vuex', () => ({
	useStore: () => mockStore,
}));

const entries =
	(hidden: string[] = [], off: string[] = []) =>
	(key: string) =>
		!hidden.includes(key) && !off.includes(key);

describe('useWorkflowMode', () => {
	beforeEach(() => {
		mockStore.state = {
			workspaceStatuses: [
				{ id: 1, type: 'default' },
				{ id: 2, type: 'completed' },
				{ id: 3, type: 'archived' },
			],
		};
		mockStore.getters['featureToggles/isFeatureEnabled'] = entries();
	});

	it('is off when Workflow is on or unknown', () => {
		expect(useWorkflowMode().simpleMode.value).toBe(false);
	});

	it('is on when Workflow is switched off or hidden', () => {
		mockStore.getters['featureToggles/isFeatureEnabled'] = entries(
			[],
			['workflow'],
		);
		expect(useWorkflowMode().simpleMode.value).toBe(true);
		mockStore.getters['featureToggles/isFeatureEnabled'] = entries([
			'workflow',
		]);
		expect(useWorkflowMode().simpleMode.value).toBe(true);
	});

	it('hides the board in simple mode only', () => {
		expect(useWorkflowMode().boardVisible.value).toBe(true);
		mockStore.getters['featureToggles/isFeatureEnabled'] = entries(
			[],
			['workflow'],
		);
		expect(useWorkflowMode().boardVisible.value).toBe(false);
	});

	it('exposes targets of the workspace statuses', () => {
		const mode = useWorkflowMode();
		expect(mode.openStatusId.value).toBe(1);
		expect(mode.doneStatusId.value).toBe(2);
		expect(mode.archiveStatusId.value).toBe(3);
		expect(mode.canMarkDone.value).toBe(true);
	});

	it('reports done and the next status of a task', () => {
		const mode = useWorkflowMode();
		expect(mode.isDoneTask({ status_id: 2 })).toBe(true);
		expect(mode.toggleTarget({ status_id: 2 })).toBe(1);
		expect(mode.toggleTarget({ status_id: 1 })).toBe(2);
	});

	it('cannot mark done without a completed status', () => {
		mockStore.state.workspaceStatuses = [{ id: 1, type: 'active' }];
		const mode = useWorkflowMode();
		expect(mode.canMarkDone.value).toBe(false);
		expect(mode.doneTooltip.value).toBe('This workspace has no Done status');
	});
});
