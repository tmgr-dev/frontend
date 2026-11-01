import {
	CloudError,
	retryMove,
	runMove,
	type MoveDeps,
	type MoveTarget,
} from '../batch';
import type { LocalRoutine } from '../payload';

const routine = (id: number, title = `Routine ${id}`): LocalRoutine => ({
	id,
	title,
	description: null,
	category: null,
	approximatelyTime: null,
	pattern: null,
	scheduledDate: null,
	scheduledTime: null,
	hasHistory: false,
});

const target: MoveTarget = { workspaceId: 7, categoryId: null, statusId: 3 };

const deps = (over: Partial<MoveDeps> = {}) => {
	const created: number[] = [];
	const removed: number[] = [];
	const value: MoveDeps = {
		create: async (r) => {
			created.push(r.id);
			return { id: 500 + r.id };
		},
		removeLocal: async (id) => {
			removed.push(id);
		},
		...over,
	};
	return { value, created, removed };
};

describe('runMove', () => {
	it('removes each local copy only after its cloud create succeeded', async () => {
		const d = deps();
		const { results, stopped } = await runMove(
			[routine(1), routine(2)],
			'routines',
			target,
			d.value,
		);
		expect(results.map((r) => r.status)).toEqual(['moved', 'moved']);
		expect(d.created).toEqual([1, 2]);
		expect(d.removed).toEqual([1, 2]);
		expect(stopped).toBeNull();
	});

	it('keeps a rejected item local and carries on with the next one', async () => {
		const d = deps({
			create: async (r) => {
				if (r.id === 1) throw new CloudError('rejected', 'Title is too long');
				return { id: 9 };
			},
		});
		const { results } = await runMove(
			[routine(1), routine(2)],
			'tasks',
			target,
			d.value,
		);
		expect(results[0]).toMatchObject({
			status: 'failed',
			message: 'Title is too long',
		});
		expect(results[1].status).toBe('moved');
		expect(d.removed).toEqual([2]);
	});

	it('stops the batch on a 401 and leaves the rest not sent', async () => {
		const attempted: number[] = [];
		const d = deps({
			create: async (r) => {
				attempted.push(r.id);
				if (r.id === 2) throw new CloudError('auth', 'Session expired');
				return { id: 9 };
			},
		});
		const { results, stopped } = await runMove(
			[routine(1), routine(2), routine(3)],
			'routines',
			target,
			d.value,
		);
		expect(attempted).toEqual([1, 2]);
		expect(results.map((r) => r.status)).toEqual([
			'moved',
			'failed',
			'not_sent',
		]);
		expect(stopped).toBe('auth');
		expect(d.removed).toEqual([1]);
	});

	it('marks an item in the cloud when the local delete fails', async () => {
		const d = deps({
			removeLocal: async () => {
				throw new Error('disk locked');
			},
		});
		const { results } = await runMove(
			[routine(1)],
			'routines',
			target,
			d.value,
		);
		expect(results[0]).toMatchObject({
			status: 'in_cloud',
			cloudId: 501,
			message: 'disk locked',
		});
	});

	it('warns that a routine may have reached the server after a network failure', async () => {
		const d = deps({
			create: async () => {
				throw new CloudError('network', 'Network Error');
			},
		});
		const { results } = await runMove(
			[routine(1)],
			'routines',
			target,
			d.value,
		);
		expect(results[0]).toMatchObject({
			status: 'failed',
			message:
				'May have reached the server. Check the workspace before retrying.',
		});
		expect(d.removed).toEqual([]);
	});

	it('keeps the plain network message for tasks, which carry an idempotency key', async () => {
		const d = deps({
			create: async () => {
				throw new CloudError('network', 'Network Error');
			},
		});
		const { results } = await runMove([routine(1)], 'tasks', target, d.value);
		expect(results[0].message).toBe('Network Error');
	});

	it('gives every item its own idempotency key', async () => {
		const keys: string[] = [];
		const d = deps({
			create: async (_r, _m, _t, key) => {
				keys.push(key);
				return { id: 1 };
			},
		});
		await runMove([routine(1), routine(2)], 'tasks', target, d.value);
		expect(new Set(keys).size).toBe(2);
	});
});

describe('retryMove', () => {
	it('retries only the local delete for an item already in the cloud and never posts again', async () => {
		let fail = true;
		const d = deps({
			removeLocal: async () => {
				if (fail) throw new Error('locked');
			},
		});
		const first = await runMove([routine(1)], 'routines', target, d.value);
		expect(first.results[0].status).toBe('in_cloud');
		fail = false;
		const second = await retryMove(
			[routine(1)],
			first.results,
			[1],
			'routines',
			target,
			d.value,
		);
		expect(second.results[0].status).toBe('moved');
		expect(d.created).toEqual([1]);
	});

	it('retries failed and not-sent items with the same key and leaves moved ones alone', async () => {
		const keys: Record<number, string[]> = {};
		let reject = true;
		const d = deps({
			create: async (r, _m, _t, key) => {
				(keys[r.id] ??= []).push(key);
				if (r.id === 2 && reject) throw new CloudError('rejected', 'No');
				return { id: 9 };
			},
		});
		const routines = [routine(1), routine(2)];
		const first = await runMove(routines, 'tasks', target, d.value);
		reject = false;
		const second = await retryMove(
			routines,
			first.results,
			[2],
			'tasks',
			target,
			d.value,
		);
		expect(second.results.map((r) => r.status)).toEqual(['moved', 'moved']);
		expect(keys[1]).toHaveLength(1);
		expect(keys[2]).toHaveLength(2);
		expect(keys[2][0]).toBe(keys[2][1]);
	});

	it('stops a retry on 401 too', async () => {
		const d = deps({
			create: async () => {
				throw new CloudError('auth', 'Session expired');
			},
		});
		const routines = [routine(1), routine(2)];
		const first = await runMove(routines, 'routines', target, d.value);
		const second = await retryMove(
			routines,
			first.results,
			[1, 2],
			'routines',
			target,
			d.value,
		);
		expect(second.stopped).toBe('auth');
		expect(second.results.map((r) => r.status)).toEqual(['failed', 'not_sent']);
	});
});
