import { matchRequest, tourBus, createTourBus, installTourInterceptor } from '../events';

const names = (events: { name: string }[]) => events.map((e) => e.name);

describe('matchRequest', () => {
	const ctx = {
		statusType: (id: number) => (id === 3 ? 'active' : undefined),
		knownStatus: (taskId: number) => (taskId === 7 ? 1 : undefined),
	};

	it.each([
		['POST', 'tasks', { data: { id: 11 } }, 'task.created', { taskId: 11 }],
		['post', '/api/tasks', { data: { id: 12 } }, 'task.created', { taskId: 12 }],
		['POST', 'tasks/5/countdown', {}, 'timer.started', { taskId: 5 }],
		['DELETE', 'tasks/5/countdown', {}, 'timer.stopped', { taskId: 5 }],
		['POST', 'tasks/5/comments', { data: { id: 2 } }, 'comment.created', { taskId: 5 }],
		['POST', 'workspaces/4/statuses', { data: { id: 9 } }, 'status.created', { workspaceId: 4 }],
		['POST', 'project_categories', { data: { id: 9 } }, 'category.created', {}],
		['POST', 'pages', { data: {} }, 'page.created', {}],
		['POST', 'personas', { data: {} }, 'persona.created', {}],
		['POST', 'personas/abc-1/tokens', {}, 'persona.token_created', {}],
		['POST', 'tasks/5/personas/abc-1', {}, 'persona.assigned', { taskId: 5 }],
		['POST', 'notify-tokens', {}, 'notify.token_created', {}],
		['POST', 'smart-device/token/generate', {}, 'mcp.token_created', {}],
		['POST', 'workspaces/4/invitations', {}, 'invite.sent', {}],
		['POST', 'tasks/5/pomodoro', {}, 'pomodoro.started', { taskId: 5 }],
		['POST', 'tasks/5/related-to/6/with/2', {}, 'relation.created', { taskId: 5 }],
		['POST', 'agent/conversations/8/messages', {}, 'ai.question_sent', {}],
		['GET', 'exports/tasks/xlsx', {}, 'export.downloaded', {}],
	])('%s %s -> %s', (method, url, response, name, payload) => {
		const events = matchRequest(method, url, undefined, response, ctx);
		expect(names(events)).toEqual([name]);
		expect(events[0].payload).toMatchObject(payload);
	});

	it('maps the board shortcut PUT tasks/{id}/{statusId}', () => {
		const [event] = matchRequest('PUT', 'tasks/7/3', undefined, {}, ctx);
		expect(event.name).toBe('task.status_changed');
		expect(event.payload).toEqual({ taskId: 7, statusId: 3, statusType: 'active' });
	});

	it('maps PUT statuses/{id}/tasks for every moved task', () => {
		const events = matchRequest('PUT', 'statuses/3/tasks', { task_ids: [1, 2] }, {}, ctx);
		expect(events.map((e) => e.payload.taskId)).toEqual([1, 2]);
		expect(events[0].payload.statusType).toBe('active');
	});

	it('emits for PUT/PATCH tasks/{id} only when status_id differs from the cached one', () => {
		expect(names(matchRequest('PUT', 'tasks/7', { status_id: 3 }, {}, ctx))).toEqual(['task.status_changed']);
		expect(names(matchRequest('PATCH', 'tasks/7', { status_id: 3 }, {}, ctx))).toEqual(['task.status_changed']);
		expect(matchRequest('PUT', 'tasks/7', { status_id: 1 }, {}, ctx)).toEqual([]);
		expect(matchRequest('PUT', 'tasks/7', { title: 'x' }, {}, ctx)).toEqual([]);
		expect(matchRequest('PUT', 'tasks/8', { status_id: 3 }, {}, ctx)).toEqual([]);
	});

	it('uses statusType null when it cannot be resolved', () => {
		const [event] = matchRequest('PUT', 'tasks/7/99', undefined, {}, ctx);
		expect(event.payload.statusType).toBeNull();
	});

	it.each([
		['PUT', 'tasks/1/settings'],
		['PUT', 'tasks/1/time'],
		['PUT', '/tasks/update-orders'],
		['POST', '/tasks/1/comments/help'],
		['POST', '/tasks/1/restore'],
		['GET', 'tasks'],
		['POST', 'tasks/1'],
		['DELETE', 'tasks/1'],
		['GET', 'tasks/1/countdown'],
		['PUT', 'tasks/1/countdown'],
		['POST', 'tasks/1/comments/2'],
		['POST', 'agent/conversations'],
		['POST', 'pages/3/restore'],
		['GET', 'exports'],
	])('does not match %s %s', (method, url) => {
		expect(matchRequest(method, url, { status_id: 3 }, { data: { id: 1 } }, ctx)).toEqual([]);
	});

	it('ignores query strings, host prefixes and trailing slashes', () => {
		expect(names(matchRequest('POST', 'https://x.test/api/tasks/5/countdown/?a=1', {}, {}))).toEqual(['timer.started']);
	});

	it('tolerates malformed bodies', () => {
		expect(matchRequest('POST', 'tasks', undefined, undefined)[0].payload.taskId).toBeNull();
		expect(matchRequest('PUT', 'statuses/3/tasks', 'oops', {}, ctx)).toEqual([]);
	});
});

describe('tourBus', () => {
	it('delivers to subscribers and unsubscribes', () => {
		const bus = createTourBus();
		const seen: unknown[] = [];
		const off = bus.on('task.created', (p, n) => seen.push([n, p]));
		bus.emit('task.created', { taskId: 1 });
		off();
		bus.emit('task.created', { taskId: 2 });
		expect(seen).toEqual([['task.created', { taskId: 1 }]]);
	});

	it('a throwing handler does not block the others', () => {
		const bus = createTourBus();
		const fn = jest.fn();
		jest.spyOn(console, 'error').mockImplementation(() => {});
		bus.on('task.created', () => {
			throw new Error('x');
		});
		bus.on('task.created', fn);
		bus.emit('task.created');
		expect(fn).toHaveBeenCalledWith({}, 'task.created');
	});

	it('exports a shared instance', () => {
		const fn = jest.fn();
		const off = tourBus.on('timer.started', fn);
		tourBus.emit('timer.started', { taskId: 1 });
		off();
		expect(fn).toHaveBeenCalledTimes(1);
	});
});

describe('installTourInterceptor', () => {
	const makeAxios = () => {
		let handler: (r: any) => any = (r) => r;
		let ejected = false;
		return {
			axios: {
				interceptors: {
					response: {
						use: (ok: (r: any) => any) => {
							handler = ok;
							return 1;
						},
						eject: () => {
							ejected = true;
						},
					},
				},
			} as any,
			run: (r: any) => handler(r),
			ejected: () => ejected,
		};
	};

	it('emits on successful responses, passes the response through and learns task statuses', () => {
		const { axios, run, ejected } = makeAxios();
		const off = installTourInterceptor(axios, (id) => (id === 3 ? 'active' : undefined));
		const seen: any[] = [];
		const stop = tourBus.on('task.status_changed', (p) => seen.push(p));
		const get = { status: 200, config: { method: 'get', url: 'tasks/7' }, data: { data: { id: 7, status_id: 1 } } };
		expect(run(get)).toBe(get);
		run({ status: 200, config: { method: 'put', url: 'tasks/7', data: JSON.stringify({ status_id: 1 }) }, data: { data: { id: 7, status_id: 1 } } });
		expect(seen).toEqual([]);
		run({ status: 200, config: { method: 'put', url: 'tasks/7', data: JSON.stringify({ status_id: 3 }) }, data: { data: { id: 7, status_id: 3 } } });
		expect(seen).toEqual([{ taskId: 7, statusId: 3, statusType: 'active' }]);
		run({ status: 200, config: { method: 'put', url: 'tasks/7', data: JSON.stringify({ status_id: 3 }) }, data: { data: { id: 7, status_id: 3 } } });
		expect(seen).toHaveLength(1);
		stop();
		off();
		expect(ejected()).toBe(true);
	});

	it('ignores non-2xx and never throws', () => {
		const { axios, run } = makeAxios();
		installTourInterceptor(axios, () => undefined);
		const fn = jest.fn();
		const stop = tourBus.on('task.created', fn);
		run({ status: 500, config: { method: 'post', url: 'tasks' }, data: { data: { id: 1 } } });
		expect(fn).not.toHaveBeenCalled();
		expect(() => run({ status: 200, config: undefined, data: null })).not.toThrow();
		run({ status: 201, config: { method: 'post', url: 'tasks' }, data: { data: { id: 1 } } });
		expect(fn).toHaveBeenCalledTimes(1);
		stop();
	});
});
