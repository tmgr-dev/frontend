import {
	buildRoutineRequest,
	buildTaskRequest,
	type LocalRoutine,
} from '../payload';

const routine = (over: Partial<LocalRoutine> = {}): LocalRoutine => ({
	id: 1000000001,
	title: 'Water plants',
	description: null,
	category: 'home',
	approximatelyTime: null,
	pattern: null,
	scheduledDate: null,
	scheduledTime: null,
	hasHistory: false,
	...over,
});

describe('buildRoutineRequest', () => {
	it('sends an undated one-off to the plain endpoint with the target workspace', () => {
		const { path, body } = buildRoutineRequest(routine(), 7);
		expect(path).toBe('daily-routines/tasks');
		expect(body).toEqual({ title: 'Water plants', workspace_id: 7 });
	});

	it('never sends the local id or fields the cloud DTO does not take', () => {
		const { body } = buildRoutineRequest(
			routine({ description: 'Ficus', approximatelyTime: 20 }),
			7,
		);
		expect(body).toEqual({
			title: 'Water plants',
			description: 'Ficus',
			workspace_id: 7,
			approximately_time: 20,
		});
		expect(JSON.stringify(body)).not.toContain('1000000001');
		expect(body).not.toHaveProperty('routine_category');
		expect(body).not.toHaveProperty('id');
	});

	it('sends a dated one-off to the recurring endpoint without a pattern, because the plain one drops the date', () => {
		const { path, body } = buildRoutineRequest(
			routine({ scheduledDate: '2026-11-03', scheduledTime: '09:30:00' }),
			7,
		);
		expect(path).toBe('daily-routines/tasks/recurring');
		expect(body).toEqual({
			title: 'Water plants',
			workspace_id: 7,
			scheduled_date: '2026-11-03',
			scheduled_time: { hours: 9, minutes: 30 },
		});
		expect(body).not.toHaveProperty('recurrence');
	});

	it('treats midnight as no time, like the local calendar does', () => {
		const { body } = buildRoutineRequest(
			routine({ scheduledDate: '2026-11-03', scheduledTime: '00:00:00' }),
			7,
		);
		expect(body).toEqual({
			title: 'Water plants',
			workspace_id: 7,
			scheduled_date: '2026-11-03',
		});
	});

	it('keeps the recurrence pattern and maps the stored columns to the DTO names', () => {
		const { path, body } = buildRoutineRequest(
			routine({
				approximatelyTime: 30,
				scheduledDate: '2026-11-03',
				pattern: {
					frequency: 'WEEKLY',
					interval: 2,
					days_of_week: ['MON', 'THU'],
					day_of_frequency: null,
					month: null,
					start_at: '2026-11-01',
					end_at: '2027-01-01',
					occurrences: 10,
					scheduled_time: '07:15:00',
					duration_min: 30,
					reminder_min: 10,
				},
			}),
			7,
		);
		expect(path).toBe('daily-routines/tasks/recurring');
		expect(body).toEqual({
			title: 'Water plants',
			workspace_id: 7,
			approximately_time: 30,
			scheduled_date: '2026-11-03',
			scheduled_time: { hours: 7, minutes: 15 },
			recurrence: {
				frequency: 'WEEKLY',
				interval: 2,
				days_of_week: ['MON', 'THU'],
				time: { hours: 7, minutes: 15 },
				duration_min: 30,
				reminder_min: 10,
				dtstart: '2026-11-01',
				dtend: '2027-01-01',
			},
		});
	});

	it('passes monthly and yearly anchors through unchanged', () => {
		const { body } = buildRoutineRequest(
			routine({
				pattern: {
					frequency: 'YEARLY',
					interval: 1,
					days_of_week: [],
					day_of_frequency: 14,
					month: 1,
					start_at: null,
					end_at: null,
					occurrences: null,
					scheduled_time: null,
					duration_min: null,
					reminder_min: null,
				},
			}),
			7,
		);
		expect(body.recurrence).toEqual({
			frequency: 'YEARLY',
			interval: 1,
			day_of_frequency: 14,
			month: 1,
		});
	});
});

describe('buildTaskRequest', () => {
	it('sends title, description, workspace, status and category only', () => {
		const body = buildTaskRequest(
			routine({
				description: 'Ficus',
				approximatelyTime: 20,
				scheduledDate: '2026-11-03',
				scheduledTime: '09:30:00',
			}),
			{ workspaceId: 7, categoryId: 12, statusId: 3 },
		);
		expect(body).toEqual({
			title: 'Water plants',
			description: 'Ficus',
			workspace_id: 7,
			status_id: 3,
			project_category_id: 12,
		});
	});

	it('omits the category when none is chosen', () => {
		const body = buildTaskRequest(routine(), {
			workspaceId: 7,
			categoryId: null,
			statusId: 3,
		});
		expect(body).toEqual({
			title: 'Water plants',
			workspace_id: 7,
			status_id: 3,
		});
	});
});
