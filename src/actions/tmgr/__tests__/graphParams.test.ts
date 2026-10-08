import {
	buildListParams,
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
});
