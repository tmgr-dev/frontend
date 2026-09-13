import { readTaskListQuery } from '../taskListQuery';

describe('task list URL state', () => {
	it('restores pagination and sorting for back/forward navigation', () => {
		expect(
			readTaskListQuery({
				page: '3',
				per_page: '25',
				sort: 'created_at',
				direction: 'asc',
			}),
		).toEqual({ page: 3, perPage: 25, sort: 'created_at', direction: 'asc' });
	});
	it('resets removed query parameters to defaults', () => {
		expect(readTaskListQuery({})).toEqual({
			page: 1,
			perPage: 10,
			sort: 'updated_at',
			direction: 'desc',
		});
	});
	it.each(['0', '-1', 'NaN', 'Infinity', '2.5', ['1', '2']])(
		'rejects invalid pagination %p',
		(value) => {
			expect(readTaskListQuery({ page: value, per_page: value })).toMatchObject(
				{ page: 1, perPage: 10 },
			);
		},
	);
	it('allows only supported sort fields and directions', () => {
		expect(
			readTaskListQuery({ sort: 'unknown', direction: 'up' }),
		).toMatchObject({ sort: 'updated_at', direction: 'desc' });
	});
});
