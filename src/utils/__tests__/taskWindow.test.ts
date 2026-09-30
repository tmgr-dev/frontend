import { isTaskWindowLabel, taskWindowTarget } from '../taskWindow';

describe('isTaskWindowLabel', () => {
	it('matches only task windows', () => {
		expect(isTaskWindowLabel('task-work-12')).toBe(true);
		expect(isTaskWindowLabel('main')).toBe(false);
		expect(isTaskWindowLabel('quick-add')).toBe(false);
		expect(isTaskWindowLabel(null)).toBe(false);
	});
});

describe('taskWindowTarget', () => {
	const workspaces = [
		{ id: 1, code: 'work' },
		{ id: -3, code: 'local-proj' },
	];
	const current = { id: 1, code: 'work' };

	it('uses the task workspace, then the current one', () => {
		expect(taskWindowTarget({ id: 5, title: ' Hi ', workspace_id: -3 }, workspaces, current)).toEqual({
			taskId: 5,
			workspaceCode: 'local-proj',
			title: 'Hi',
		});
		expect(taskWindowTarget({ id: 5, title: 'x' }, workspaces, { id: 9, code: 'cur' })).toEqual({
			taskId: 5,
			workspaceCode: 'cur',
			title: 'x',
		});
	});

	it('gives a null title for blank titles', () => {
		expect(taskWindowTarget({ id: 5, title: '  ' }, workspaces, current)?.title).toBeNull();
	});

	it('is null without an id or a workspace code', () => {
		expect(taskWindowTarget({ title: 'x' }, workspaces, current)).toBeNull();
		expect(taskWindowTarget({ id: 5 }, [], null)).toBeNull();
		expect(taskWindowTarget(null, workspaces, current)).toBeNull();
	});
});
