import {
	readNtfySubscribed,
	syncNtfySubscription,
	writeNtfySubscribed,
} from '@/utils/ntfySubscription';

const memoryStorage = () => {
	const data = new Map<string, string>();
	return {
		getItem: (key: string) => data.get(key) ?? null,
		setItem: (key: string, value: string) => void data.set(key, value),
		removeItem: (key: string) => void data.delete(key),
		data,
	};
};

const throwingStorage = {
	getItem: () => {
		throw new Error('denied');
	},
	setItem: () => {
		throw new Error('denied');
	},
	removeItem: () => {
		throw new Error('denied');
	},
};

const fakeClient = () => ({
	start: jest.fn().mockResolvedValue(undefined),
	stop: jest.fn().mockResolvedValue(undefined),
	setUserId: jest.fn().mockResolvedValue(undefined),
});

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('opt-in flag', () => {
	test('is stored per user and missing means off', () => {
		const storage = memoryStorage();
		expect(readNtfySubscribed(storage, 7)).toBe(false);
		writeNtfySubscribed(storage, 7, true);
		expect(storage.data.get('tmgr:ntfy-subscribed:7')).toBe('1');
		expect(readNtfySubscribed(storage, '7')).toBe(true);
		expect(readNtfySubscribed(storage, 8)).toBe(false);
		writeNtfySubscribed(storage, 7, false);
		expect(storage.data.has('tmgr:ntfy-subscribed:7')).toBe(false);
	});

	test('no user, no storage or a throwing storage reads as off without throwing', () => {
		const storage = memoryStorage();
		writeNtfySubscribed(storage, undefined, true);
		expect(storage.data.size).toBe(0);
		expect(readNtfySubscribed(storage, undefined)).toBe(false);
		expect(readNtfySubscribed(null, 7)).toBe(false);
		expect(readNtfySubscribed(throwingStorage, 7)).toBe(false);
		expect(() => writeNtfySubscribed(throwingStorage, 7, true)).not.toThrow();
		expect(() => writeNtfySubscribed(null, 7, true)).not.toThrow();
	});
});

describe('syncNtfySubscription', () => {
	test('reload with the flag on reconnects for that user', async () => {
		const storage = memoryStorage();
		writeNtfySubscribed(storage, 7, true);
		const client = fakeClient();
		const tokenProvider = {};
		const id = syncNtfySubscription({
			client,
			storage,
			connectedUserId: null,
			userId: 7,
			tokenProvider,
		});
		expect(id).toBe('7');
		await flush();
		expect(client.start).toHaveBeenCalled();
		expect(client.setUserId).toHaveBeenCalledWith('7', tokenProvider);
		expect(client.stop).not.toHaveBeenCalled();
	});

	test.each([
		['missing', () => memoryStorage()],
		[
			'off',
			() => {
				const storage = memoryStorage();
				writeNtfySubscribed(storage, 7, true);
				writeNtfySubscribed(storage, 7, false);
				return storage;
			},
		],
		['unreadable', () => throwingStorage],
	])('reload with the flag %s connects nothing', async (_, makeStorage) => {
		const client = fakeClient();
		const id = syncNtfySubscription({
			client,
			storage: makeStorage(),
			connectedUserId: null,
			userId: 7,
		});
		expect(id).toBeNull();
		await flush();
		expect(client.start).not.toHaveBeenCalled();
		expect(client.setUserId).not.toHaveBeenCalled();
		expect(client.stop).not.toHaveBeenCalled();
	});

	test('no user (guest, logged out, excluded window) connects nothing', async () => {
		const storage = memoryStorage();
		writeNtfySubscribed(storage, 7, true);
		const client = fakeClient();
		expect(
			syncNtfySubscription({
				client,
				storage,
				connectedUserId: null,
				userId: null,
			}),
		).toBeNull();
		await flush();
		expect(client.start).not.toHaveBeenCalled();
		expect(client.setUserId).not.toHaveBeenCalled();
	});

	test('already connected for the same user is a no-op', async () => {
		const storage = memoryStorage();
		writeNtfySubscribed(storage, 7, true);
		const client = fakeClient();
		expect(
			syncNtfySubscription({
				client,
				storage,
				connectedUserId: '7',
				userId: 7,
			}),
		).toBe('7');
		await flush();
		expect(client.start).not.toHaveBeenCalled();
		expect(client.setUserId).not.toHaveBeenCalled();
		expect(client.stop).not.toHaveBeenCalled();
	});

	test('account switch: A opted in, B is not connected; A again reconnects', async () => {
		const storage = memoryStorage();
		writeNtfySubscribed(storage, 1, true);
		const client = fakeClient();

		let connected = syncNtfySubscription({
			client,
			storage,
			connectedUserId: null,
			userId: 1,
		});
		expect(connected).toBe('1');
		await flush();
		expect(client.setUserId).toHaveBeenLastCalledWith('1', undefined);

		connected = syncNtfySubscription({
			client,
			storage,
			connectedUserId: connected,
			userId: 2,
		});
		expect(connected).toBeNull();
		await flush();
		expect(client.stop).toHaveBeenCalledTimes(1);
		expect(client.setUserId).toHaveBeenCalledTimes(1);

		connected = syncNtfySubscription({
			client,
			storage,
			connectedUserId: connected,
			userId: 1,
		});
		expect(connected).toBe('1');
		await flush();
		expect(client.setUserId).toHaveBeenCalledTimes(2);
		expect(client.setUserId).toHaveBeenLastCalledWith('1', undefined);
		expect(client.stop).toHaveBeenCalledTimes(1);
	});

	test('switching from one opted-in user to another stops, then connects the new one', async () => {
		const storage = memoryStorage();
		writeNtfySubscribed(storage, 1, true);
		writeNtfySubscribed(storage, 2, true);
		const client = fakeClient();
		expect(
			syncNtfySubscription({
				client,
				storage,
				connectedUserId: '1',
				userId: 2,
			}),
		).toBe('2');
		await flush();
		expect(client.stop).toHaveBeenCalledTimes(1);
		expect(client.setUserId).toHaveBeenCalledWith('2', undefined);
		expect(client.stop.mock.invocationCallOrder[0]).toBeLessThan(
			client.setUserId.mock.invocationCallOrder[0],
		);
	});
});
