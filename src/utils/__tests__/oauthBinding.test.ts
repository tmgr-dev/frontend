import { createOAuthBinding, takeOAuthBinding } from '@/utils/oauthBinding';

beforeAll(() => {
	if (!globalThis.crypto?.getRandomValues) {
		Object.defineProperty(globalThis, 'crypto', {
			value: require('node:crypto').webcrypto,
			configurable: true,
		});
	}
});

beforeEach(() => {
	const store = new Map<string, string>();
	(global as any).sessionStorage = {
		getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
		setItem: (key: string, value: string) => store.set(key, String(value)),
		removeItem: (key: string) => store.delete(key),
	};
});

afterAll(() => {
	delete (global as any).sessionStorage;
});

describe('oauthBinding', () => {
	it('creates a 43-char base64url binding without padding', () => {
		expect(createOAuthBinding('github')).toMatch(/^[A-Za-z0-9_-]{43}$/);
	});

	it('stores bindings independently per provider', () => {
		const github = createOAuthBinding('github');
		const google = createOAuthBinding('google');

		expect(github).not.toBe(google);
		expect(takeOAuthBinding('google')).toBe(google);
		expect(takeOAuthBinding('github')).toBe(github);
	});

	it('returns the stored binding once and then removes it', () => {
		const binding = createOAuthBinding('github');

		expect(takeOAuthBinding('github')).toBe(binding);
		expect(takeOAuthBinding('github')).toBeUndefined();
	});

	it('still returns a binding when storage is unavailable', () => {
		(global as any).sessionStorage = {
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

		expect(createOAuthBinding('google')).toMatch(/^[A-Za-z0-9_-]{43}$/);
		expect(takeOAuthBinding('google')).toBeUndefined();
	});
});
