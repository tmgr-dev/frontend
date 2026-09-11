import { parseStoredToken, tokenFromStorageEvent } from '../tokenSync';

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
		expect(tokenFromStorageEvent({ key: 'token', newValue: null }, current)).toEqual({
			changed: true,
			token: null,
		});
	});

	it('reports no change when both sides are logged out', () => {
		expect(tokenFromStorageEvent({ key: 'token', newValue: null }, null)).toEqual({
			changed: false,
		});
	});
});
