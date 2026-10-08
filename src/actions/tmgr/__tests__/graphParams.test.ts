import {
	buildListParams,
	buildMapParams,
	buildPathParams,
	buildRelatedParams,
} from '../graphParams';

describe('graph params', () => {
	it('joins include and drops empty values', () => {
		expect(
			buildRelatedParams({
				entity: 'task:12',
				depth: 2,
				include: ['tasks', 'pages'],
				workspace_id: null,
			}),
		).toEqual({ entity: 'task:12', depth: 2, include: 'tasks,pages' });
	});

	it('keeps workspace and limit when given', () => {
		expect(
			buildRelatedParams({ entity: 'page:x', limit: 50, workspace_id: 3 }),
		).toEqual({ entity: 'page:x', limit: 50, workspace_id: 3 });
	});

	it('builds path and list params', () => {
		expect(
			buildPathParams({ from: 'task:1', to: 'page:2', max_depth: 4 }),
		).toEqual({ from: 'task:1', to: 'page:2', max_depth: 4 });
		expect(buildListParams({ workspace_id: 5, limit: 10 })).toEqual({
			workspace_id: 5,
			limit: 10,
		});
	});

	it('builds map params and drops an open range', () => {
		expect(
			buildMapParams({
				workspace_id: 5,
				from: '2026-09-08',
				to: '2026-10-08',
				limit: 3000,
			}),
		).toEqual({
			workspace_id: 5,
			from: '2026-09-08',
			to: '2026-10-08',
			limit: 3000,
		});
		expect(buildMapParams({ workspace_id: 5, from: null, to: '' })).toEqual({
			workspace_id: 5,
		});
	});
});
