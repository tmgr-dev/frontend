import { shouldReloadTaskPages } from '../taskMentions';

describe('shouldReloadTaskPages', () => {
	it('reloads when the event lists this task', () => {
		expect(
			shouldReloadTaskPages(
				{ type: 'page.updated', page: { linked_task_ids: [3, 5] } },
				5,
			),
		).toBe(true);
	});

	it('ignores other tasks, local events and missing payloads', () => {
		expect(
			shouldReloadTaskPages(
				{ type: 'page.updated', page: { linked_task_ids: [3] } },
				5,
			),
		).toBe(false);
		expect(shouldReloadTaskPages({ type: 'page.local', page: null }, 5)).toBe(
			false,
		);
		expect(shouldReloadTaskPages(null, 5)).toBe(false);
		expect(shouldReloadTaskPages({ type: 'page.updated', page: {} }, 5)).toBe(
			false,
		);
	});
});
