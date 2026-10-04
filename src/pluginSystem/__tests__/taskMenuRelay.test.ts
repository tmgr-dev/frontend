import type { TaskMenuItem } from '../taskMenu';
import {
	TASK_MENU_ITEMS,
	TASK_MENU_REQUEST,
	TASK_MENU_RESULT,
	TASK_MENU_RUN,
	TASK_MENU_RUN_TIMEOUT_MS,
	createTaskMenuRelayClient,
	createTaskMenuRelayHost,
	parseTaskMenuItems,
	parseTaskMenuRun,
} from '../taskMenuRelay';

const item: TaskMenuItem = {
	pluginId: 'dev.a',
	pluginName: 'A',
	command: 'dev.a.go',
	title: 'Go',
};

const wire = (
	items: TaskMenuItem[],
	run = jest.fn().mockResolvedValue(undefined),
	timeoutMs = 50,
	windowWorkspace = 5,
	mainWorkspace: number | null = 5,
) => {
	let host!: ReturnType<typeof createTaskMenuRelayHost>;
	let client!: ReturnType<typeof createTaskMenuRelayClient>;
	const log: { target: string | null; channel: string; payload: any }[] = [];
	const deliver = (target: string | null, channel: string, payload: any) => {
		log.push({ target, channel, payload });
		if (channel === TASK_MENU_REQUEST) host.onRequest(payload);
		if (channel === TASK_MENU_RUN) void host.onRun(payload);
		if (channel === TASK_MENU_ITEMS) client.onItems(payload);
		if (channel === TASK_MENU_RESULT) client.onResult(payload);
	};
	host = createTaskMenuRelayHost({
		send: deliver,
		getItems: () => items,
		getWorkspaceId: () => mainWorkspace,
		run,
	});
	client = createTaskMenuRelayClient({
		label: 'task-7',
		send: deliver,
		workspaceId: () => windowWorkspace,
		timeoutMs,
		nextId: (() => {
			let n = 0;
			return () => `r${++n}`;
		})(),
	});
	return { host, client, log, run };
};

describe('payload parsing', () => {
	it('accepts a well-formed item list and rejects malformed ones', () => {
		expect(parseTaskMenuItems({ items: [item] })).toEqual([item]);
		expect(parseTaskMenuItems({ items: [{ ...item, title: '' }] })).toBeNull();
		expect(parseTaskMenuItems({ items: [{ ...item, command: 5 }] })).toBeNull();
		expect(parseTaskMenuItems({ items: 'x' })).toBeNull();
		expect(parseTaskMenuItems(null)).toBeNull();
	});

	it('requires a positive integer task id and string fields on run', () => {
		const ok = {
			requestId: 'r',
			label: 'task-1',
			pluginId: 'p',
			command: 'c',
			taskId: 3,
			workspaceId: 5,
		};
		expect(parseTaskMenuRun(ok)).toEqual(ok);
		for (const taskId of [0, -1, 1.5, '3', null, NaN])
			expect(parseTaskMenuRun({ ...ok, taskId })).toBeNull();
		for (const workspaceId of [0, undefined, '5', null])
			expect(parseTaskMenuRun({ ...ok, workspaceId })).toBeNull();
		expect(parseTaskMenuRun({ ...ok, pluginId: undefined })).toBeNull();
		expect(parseTaskMenuRun([])).toBeNull();
	});
});

describe('relay round trip', () => {
	it('starts hidden and shows items after the snapshot request is answered', async () => {
		const { client, log } = wire([item]);
		expect(client.items.value).toEqual([]);
		await client.requestSnapshot();
		expect(log[0]).toEqual({
			target: 'main',
			channel: TASK_MENU_REQUEST,
			payload: { label: 'task-7' },
		});
		expect(log[1]).toMatchObject({
			target: 'task-7',
			channel: TASK_MENU_ITEMS,
		});
		expect(client.items.value).toEqual([item]);
	});

	it('stays hidden when the main window never answers', async () => {
		const client = createTaskMenuRelayClient({
			label: 'task-7',
			workspaceId: () => 5,
			send: () => undefined,
		});
		await client.requestSnapshot();
		expect(client.items.value).toEqual([]);
	});

	it('shows an empty list when the main host is not ready', async () => {
		const { client } = wire([]);
		await client.requestSnapshot();
		expect(client.items.value).toEqual([]);
	});

	it('applies broadcast snapshots and ignores malformed ones', () => {
		const { host, client } = wire([item]);
		host.broadcast();
		expect(client.items.value).toEqual([item]);
		client.onItems({ items: [{ nope: 1 }] });
		expect(client.items.value).toEqual([item]);
	});

	it('runs a command in the main window and resolves on a null error', async () => {
		const { client, run, log } = wire([item]);
		await client.run(item, 9);
		expect(run).toHaveBeenCalledWith('dev.a', 'dev.a.go', 9, 5);
		expect(log[0]).toEqual({
			target: 'main',
			channel: TASK_MENU_RUN,
			payload: {
				requestId: 'r1',
				label: 'task-7',
				pluginId: 'dev.a',
				command: 'dev.a.go',
				taskId: 9,
				workspaceId: 5,
			},
		});
		expect(log[1]).toEqual({
			target: 'task-7',
			channel: TASK_MENU_RESULT,
			payload: { requestId: 'r1', error: null },
		});
	});

	it('rejects with the main window error message', async () => {
		const { client } = wire(
			[item],
			jest.fn().mockRejectedValue(new Error('denied')),
		);
		await expect(client.run(item, 1)).rejects.toThrow('denied');
	});

	it('ignores a result with another request id and times out', async () => {
		const client = createTaskMenuRelayClient({
			label: 'task-7',
			workspaceId: () => 5,
			send: () => undefined,
			timeoutMs: 20,
		});
		const pending = client.run(item, 1);
		client.onResult({ requestId: 'other', error: null });
		await expect(pending).rejects.toThrow('did not answer');
	});

	it('does not run on an invalid request and sends no result', async () => {
		const { host, run, log } = wire([item]);
		await host.onRun({
			requestId: 'r',
			label: 'task-7',
			pluginId: 'p',
			command: 'c',
			taskId: -4,
			workspaceId: 5,
		});
		expect(run).not.toHaveBeenCalled();
		expect(log).toHaveLength(0);
	});

	it('rejects when sending fails', async () => {
		const client = createTaskMenuRelayClient({
			label: 'task-7',
			workspaceId: () => 5,
			send: () => Promise.reject(new Error('no bridge')),
		});
		await expect(client.run(item, 1)).rejects.toThrow('no bridge');
	});

	it('hides items snapshotted for another workspace than the window shows', async () => {
		const { client } = wire([item], undefined, 50, 6, 5);
		await client.requestSnapshot();
		expect(client.items.value).toEqual([]);
	});

	it('hides items when the main window runs plugins in no workspace', async () => {
		const { client } = wire([item], undefined, 50, 5, null);
		await client.requestSnapshot();
		expect(client.items.value).toEqual([]);
	});

	it('refuses to run when the window has no workspace', async () => {
		const send = jest.fn();
		const client = createTaskMenuRelayClient({
			label: 'task-7',
			workspaceId: () => null,
			send,
		});
		await expect(client.run(item, 1)).rejects.toThrow('not available');
		expect(send).not.toHaveBeenCalled();
	});

	it('waits longer than a plugin may wait on the user', () => {
		expect(TASK_MENU_RUN_TIMEOUT_MS).toBeGreaterThan(5 * 60_000);
	});
});
