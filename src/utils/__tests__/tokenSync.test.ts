import {
	parseStoredToken,
	tokenFromStorageEvent,
	tokenSubject,
} from '../tokenSync';

const fakeJwt = (payload: unknown) =>
	[
		Buffer.from('{"alg":"none"}').toString('base64url'),
		Buffer.from(JSON.stringify(payload)).toString('base64url'),
		'sig',
	].join('.');

describe('parseStoredToken', () => {
	it('parses a persisted envelope', () => {
		expect(parseStoredToken('{"token":"a","refresh_token":"r"}')).toEqual({
			token: 'a',
			refresh_token: 'r',
		});
	});

	it.each([null, '', 'not json', '"just a string"', '42'])(
		'returns null for %p',
		(raw) => expect(parseStoredToken(raw)).toBeNull(),
	);
});

describe('tokenFromStorageEvent', () => {
	const current = { token: 'a', refresh_token: 'r1' };

	it('ignores other keys', () => {
		expect(
			tokenFromStorageEvent({ key: 'theme', newValue: 'dark' }, current),
		).toEqual({ changed: false });
	});

	it('adopts a token rotated by another tab', () => {
		expect(
			tokenFromStorageEvent(
				{ key: 'token', newValue: '{"token":"b","refresh_token":"r2"}' },
				current,
			),
		).toEqual({ changed: true, token: { token: 'b', refresh_token: 'r2' } });
	});

	it('reports no change when the other tab wrote the same tokens', () => {
		expect(
			tokenFromStorageEvent(
				{ key: 'token', newValue: '{"token":"a","refresh_token":"r1"}' },
				current,
			),
		).toEqual({ changed: false });
	});

	it('clears the session when another tab logged out', () => {
		expect(
			tokenFromStorageEvent({ key: 'token', newValue: null }, current),
		).toEqual({
			changed: true,
			token: null,
		});
	});

	it('reports no change when both sides are logged out', () => {
		expect(
			tokenFromStorageEvent({ key: 'token', newValue: null }, null),
		).toEqual({
			changed: false,
		});
	});
});

describe('tokenSubject', () => {
	it('reads a numeric string subject', () => {
		expect(tokenSubject({ token: fakeJwt({ sub: '42' }) } as any)).toBe(42);
	});

	it('reads a numeric subject', () => {
		expect(tokenSubject({ token: fakeJwt({ sub: 7 }) } as any)).toBe(7);
	});

	it('decodes base64url payloads with - and _ characters', () => {
		const payload = { sub: '9', note: '\u00ff\u00fe?>>>???' };
		const encoded = Buffer.from(JSON.stringify(payload)).toString('base64url');
		expect(/[-_]/.test(encoded)).toBe(true);
		expect(tokenSubject({ token: `h.${encoded}.s` } as any)).toBe(9);
	});

	it.each(['', 'opaque-token', 'a.b', 'a.!!!.c', 'a..c'])(
		'returns null for non-JWT %p',
		(token) => expect(tokenSubject({ token } as any)).toBeNull(),
	);

	it.each(['abc', '4x', '1.5', '', '0', '-3', 0, -1, 1.5, null, {}])(
		'returns null for subject %p',
		(sub) =>
			expect(tokenSubject({ token: fakeJwt({ sub }) } as any)).toBeNull(),
	);

	it('returns null when the payload is not an object', () => {
		expect(tokenSubject({ token: fakeJwt('text') } as any)).toBeNull();
		expect(tokenSubject({ token: fakeJwt(null) } as any)).toBeNull();
	});

	it.each([null, undefined])('returns null for %p', (token) =>
		expect(tokenSubject(token)).toBeNull(),
	);
});
