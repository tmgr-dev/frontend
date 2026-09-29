import { consumeSessionExpired, markSessionExpired } from '../sessionExpiry';

const memoryStorage = () => {
	const data = new Map<string, string>();
	return {
		getItem: (key: string) => (data.has(key) ? data.get(key)! : null),
		setItem: (key: string, value: string) => {
			data.set(key, value);
		},
		removeItem: (key: string) => {
			data.delete(key);
		},
	};
};

const throwingStorage = () => ({
	getItem: () => {
		throw new Error('storage disabled');
	},
	setItem: () => {
		throw new Error('storage disabled');
	},
	removeItem: () => {
		throw new Error('storage disabled');
	},
});

describe('sessionExpiry', () => {
	beforeEach(() => {
		(global as any).sessionStorage = memoryStorage();
	});

	afterAll(() => {
		delete (global as any).sessionStorage;
	});

	it('reports an expired session exactly once', () => {
		markSessionExpired();
		expect(consumeSessionExpired()).toBe(true);
		expect(consumeSessionExpired()).toBe(false);
	});

	it('reports nothing when no session was marked expired', () => {
		expect(consumeSessionExpired()).toBe(false);
	});

	it('never throws when storage is unavailable', () => {
		(global as any).sessionStorage = throwingStorage();
		expect(() => markSessionExpired()).not.toThrow();
		expect(consumeSessionExpired()).toBe(false);
	});
});
