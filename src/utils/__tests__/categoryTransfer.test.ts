import { describeCategoryTransfer } from '@/utils/categoryTransfer';

describe('describeCategoryTransfer', () => {
	it('summarises tasks, remapped statuses and dropped assignees', () => {
		const text = describeCategoryTransfer({
			category: { id: 1, title: 'Backend', workspace_id: 2 },
			tasks_moved: 5,
			assignees_removed: 1,
			status_mappings: [
				{ from_id: 10, from_name: 'In progress', to_id: 20, to_name: 'Doing', tasks: 3 },
				{ from_id: 11, from_name: 'QA', to_id: 20, to_name: 'Doing', tasks: 1 },
			],
		});

		expect(text).toBe(
			'5 tasks moved. Statuses: In progress → Doing (3), QA → Doing (1). 1 assignee removed (not a member of the target workspace).',
		);
	});

	it('keeps it short when nothing had to change', () => {
		expect(
			describeCategoryTransfer({
				category: { id: 1, title: 'Backend', workspace_id: 2 },
				tasks_moved: 1,
				assignees_removed: 0,
				status_mappings: [],
			}),
		).toBe('1 task moved.');
	});

	it('explains a status that could not be mapped', () => {
		expect(
			describeCategoryTransfer({
				category: { id: 1, title: 'Backend', workspace_id: 2 },
				tasks_moved: 2,
				assignees_removed: 2,
				status_mappings: [{ from_id: 10, from_name: 'Odd', to_id: null, to_name: null, tasks: 2 }],
			}),
		).toBe('2 tasks moved. Statuses: Odd → no status (2). 2 assignees removed (not members of the target workspace).');
	});
});
