import { createFollowersStore, type FollowersApi } from '../followersStore';
import type { FollowerInfo, FollowerResult, ManagerMachine } from '../types';

const info = (follower: string, chat_enroll: boolean | null = true) =>
	({ follower, chat_enroll }) as FollowerInfo;

const setup = (
	followers: FollowerInfo[],
	connect: FollowerResult[] | null | Error = [],
) => {
	const api: FollowersApi = {
		followers: jest.fn(async () => followers),
		connect: jest.fn(async () => {
			if (connect instanceof Error) throw connect;
			return connect;
		}),
	};
	const onDone = jest.fn(async () => undefined);
	return { api, onDone, store: createFollowersStore(api, onDone) };
};

const machine = (label: string): ManagerMachine => ({
	machine: `mx_${label}`,
	label,
	role: 'sub',
});

describe('followers store', () => {
	it('counts followers that are not yet machines, ignoring case', async () => {
		const { store } = setup([info('Studio'), info('server'), info('nas')]);
		await store.refresh();
		expect(store.pendingCount([machine('studio'), machine('Other')])).toBe(2);
		expect(store.pendingCount([])).toBe(3);
	});

	it('stays empty when the command is unavailable or fails', async () => {
		const { api, store } = setup([]);
		await store.refresh();
		expect(store.followers).toEqual([]);
		(api.followers as jest.Mock).mockRejectedValueOnce(new Error('boom'));
		await store.refresh();
		expect(store.followers).toEqual([]);
	});

	it('connect stores the results, remembers connected names and calls onDone', async () => {
		const results: FollowerResult[] = [
			{ follower: 'a', state: 'enrolled', machine: 'mx_a' },
			{ follower: 'b', state: 'refused', message: 'x' },
			{ follower: 'c', state: 'already' },
		];
		const { store, onDone } = setup([info('a'), info('b'), info('c')], results);
		await store.refresh();
		await store.connect();
		expect(store.results).toEqual(results);
		expect(store.busy).toBe(false);
		expect(store.pendingCount([])).toBe(1);
		expect(onDone).toHaveBeenCalledTimes(1);
	});

	it('a rejected connect becomes one error line and does not throw', async () => {
		const { store, onDone } = setup([info('a')], new Error('nope'));
		await store.refresh();
		await store.connect();
		expect(store.results).toEqual([
			{ follower: '', state: 'error', message: 'nope' },
		]);
		expect(store.busy).toBe(false);
		expect(onDone).not.toHaveBeenCalled();
	});

	it('a missing command yields no results', async () => {
		const { store } = setup([info('a')], null);
		await store.refresh();
		await store.connect();
		expect(store.results).toEqual([]);
	});

	it('ignores a second connect while one runs', async () => {
		const { api, store } = setup([info('a')]);
		await Promise.all([store.connect(), store.connect()]);
		expect(api.connect).toHaveBeenCalledTimes(1);
	});

	it('lists followers whose scope refuses chat enroll as hints', async () => {
		const { store } = setup([info('a'), info('b', false), info('c', null)]);
		await store.refresh();
		expect(store.scopeHints).toEqual(['b']);
	});
});
