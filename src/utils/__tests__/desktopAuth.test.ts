import { webcrypto } from 'node:crypto';
import {
	PENDING_TTL_MS,
	beginDesktopAuth,
	buildDesktopCallbackUrl,
	createPkcePair,
	desktopTxFromState,
	parseAuthCallback,
	parseRelayFragment,
	relayReturnHash,
	takePendingDesktopAuth,
} from '../desktopAuth';

const memoryStorage = () => {
	const data = new Map<string, string>();
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
		removeItem: (k: string) => void data.delete(k),
		data,
	};
};

const cryptoImpl = webcrypto as unknown as Crypto;
const CODE = 'c'.repeat(43);
const STATE = 's'.repeat(43);

const startedAt = async (
	storage: ReturnType<typeof memoryStorage>,
	now: number,
) => {
	const url = await beginDesktopAuth(
		'github',
		'https://api.example/api/',
		storage,
		() => now,
		cryptoImpl,
	);
	return new URL(url);
};

describe('createPkcePair', () => {
	it('derives the S256 challenge from the verifier (RFC 7636 appendix B)', async () => {
		const fixed = {
			getRandomValues: (bytes: Uint8Array) => {
				bytes.set([
					116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173,
					187, 186, 22, 212, 37, 77, 105, 214, 191, 240, 91, 88, 5, 88, 83,
					132, 141, 121,
				]);
				return bytes;
			},
			subtle: cryptoImpl.subtle,
		} as unknown as Crypto;

		expect(await createPkcePair(fixed)).toEqual({
			verifier: 'dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk',
			challenge: 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM',
		});
	});

	it('fails loudly without WebCrypto instead of hanging', async () => {
		await expect(createPkcePair({} as Crypto)).rejects.toThrow(
			'secure context',
		);
	});
});

describe('beginDesktopAuth', () => {
	it('builds the backend start URL and keeps the verifier only locally', async () => {
		const storage = memoryStorage();
		const url = await startedAt(storage, 1000);

		expect(url.origin + url.pathname).toBe(
			'https://api.example/api/auth/login/desktop/github',
		);
		const pending = JSON.parse([...storage.data.values()][0]);
		expect(url.searchParams.get('state')).toBe(pending.state);
		expect(url.searchParams.get('code_challenge')).toMatch(
			/^[A-Za-z0-9_-]{43}$/,
		);
		expect(url.toString()).not.toContain(pending.verifier);
	});
});

describe('takePendingDesktopAuth', () => {
	it('returns the pending attempt once for the matching state', async () => {
		const storage = memoryStorage();
		const state = (await startedAt(storage, 1000)).searchParams.get('state')!;

		expect(takePendingDesktopAuth(state, storage, () => 2000)).toMatchObject({
			provider: 'github',
			state,
		});
		expect(takePendingDesktopAuth(state, storage, () => 2000)).toBeNull();
	});

	it('ignores a foreign state and keeps the current attempt', async () => {
		const storage = memoryStorage();
		const state = (await startedAt(storage, 1000)).searchParams.get('state')!;

		expect(takePendingDesktopAuth(STATE, storage, () => 2000)).toBeNull();
		expect(takePendingDesktopAuth(state, storage, () => 2000)).not.toBeNull();
	});

	it('a newer attempt replaces the old one, so a late callback of the old one is rejected', async () => {
		const storage = memoryStorage();
		const oldState = (await startedAt(storage, 1000)).searchParams.get('state')!;
		const newState = (await startedAt(storage, 2000)).searchParams.get('state')!;

		expect(takePendingDesktopAuth(oldState, storage, () => 3000)).toBeNull();
		expect(takePendingDesktopAuth(newState, storage, () => 3000)).not.toBeNull();
	});

	it('rejects and clears an expired attempt', async () => {
		const storage = memoryStorage();
		const state = (await startedAt(storage, 1000)).searchParams.get('state')!;

		expect(
			takePendingDesktopAuth(state, storage, () => 1001 + PENDING_TTL_MS),
		).toBeNull();
		expect(storage.data.size).toBe(0);
	});

	it('survives corrupted storage', () => {
		const storage = memoryStorage();
		storage.setItem('desktop.auth.pending', '{not json');

		expect(takePendingDesktopAuth(STATE, storage, () => 0)).toBeNull();
	});
});

describe('parseAuthCallback', () => {
	it('parses a success callback', () => {
		expect(
			parseAuthCallback(`tmgr://auth/callback?code=${CODE}&state=${STATE}`),
		).toEqual({ type: 'auth', code: CODE, state: STATE });
	});

	it('parses an error callback', () => {
		expect(parseAuthCallback('tmgr://auth/callback?error=apple')).toEqual({
			type: 'auth',
			error: 'apple',
		});
	});

	it.each([
		'tmgr://auth/callback',
		`tmgr://auth/callback?code=${CODE}`,
		`tmgr://auth/callback?code=short&state=${STATE}`,
		`tmgr://auth/callback?code=${CODE}&state=bad%20state`,
		'tmgr://auth/callback?error=<script>',
		`tmgr://auth/callbackx?code=${CODE}&state=${STATE}`,
		`https://auth/callback?code=${CODE}&state=${STATE}`,
		'tmgr://task/12',
	])('rejects %s', (url) => {
		expect(parseAuthCallback(url)).toBeNull();
	});
});

describe('website relay helpers', () => {
	it('extracts the transaction only from a desktop state', () => {
		expect(desktopTxFromState(`desktop.${CODE}`)).toBe(CODE);
		expect(desktopTxFromState('mobile')).toBeNull();
		expect(desktopTxFromState(undefined)).toBeNull();
		expect(desktopTxFromState(`desktop.${CODE}&x=1`)).toBeNull();
		expect(desktopTxFromState([`desktop.${CODE}`])).toBeNull();
	});

	it('turns the return-page fragment into the app deep link', () => {
		const result = parseRelayFragment(`#code=${CODE}&state=${STATE}`);
		expect(result).toEqual({ code: CODE, state: STATE });
		expect(buildDesktopCallbackUrl(result!)).toBe(
			`tmgr://auth/callback?code=${CODE}&state=${STATE}`,
		);
		expect(buildDesktopCallbackUrl({ error: 'apple' })).toBe(
			'tmgr://auth/callback?error=apple',
		);
	});

	it('relayReturnHash round-trips through parseRelayFragment', () => {
		expect(
			parseRelayFragment(relayReturnHash({ code: CODE, state: STATE })),
		).toEqual({ code: CODE, state: STATE });
		expect(parseRelayFragment(relayReturnHash({ error: 'telegram' }))).toEqual(
			{ error: 'telegram' },
		);
	});

	it('rejects a malformed fragment instead of forwarding it', () => {
		expect(parseRelayFragment('')).toBeNull();
		expect(parseRelayFragment('#code=x&state=y')).toBeNull();
		expect(parseRelayFragment('#error=javascript:alert(1)')).toBeNull();
	});
});
