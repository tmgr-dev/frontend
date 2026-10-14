import { apiErrorText } from '../apiErrorText';

const withData = (data: unknown) => ({ response: { data } });

describe('apiErrorText', () => {
	it('reads message from a Java ApiError body', () => {
		expect(apiErrorText(withData({ message: 'Repo not found' }), 'x')).toBe(
			'Repo not found',
		);
		expect(
			apiErrorText(
				withData({ message: 'The given data was invalid.', errors: {} }),
				'x',
			),
		).toBe('The given data was invalid.');
	});

	it('falls back to the legacy error field', () => {
		expect(apiErrorText(withData({ error: 'Invalid API key' }), 'x')).toBe(
			'Invalid API key',
		);
	});

	it('prefers message over error', () => {
		expect(
			apiErrorText(withData({ message: 'Readable', error: 'code' }), 'x'),
		).toBe('Readable');
	});

	it('trims the returned text', () => {
		expect(apiErrorText(withData({ message: '  Repo gone \n' }), 'x')).toBe(
			'Repo gone',
		);
		expect(apiErrorText(withData({ error: '\tBad key ' }), 'x')).toBe(
			'Bad key',
		);
	});

	it('skips blank or non-string message', () => {
		expect(apiErrorText(withData({ message: '  ', error: 'Old' }), 'x')).toBe(
			'Old',
		);
		expect(
			apiErrorText(withData({ message: { a: 1 }, error: 'Old' }), 'x'),
		).toBe('Old');
	});

	it.each([
		['undefined error', undefined],
		['null error', null],
		['string error', 'boom'],
		['no response', new Error('Network Error')],
		['null data', withData(null)],
		['empty data', withData({})],
		['html data', withData('<html>502 Bad Gateway</html>')],
		['empty string data', withData('')],
		['array data', withData(['a'])],
		['numeric message', withData({ message: 500 })],
		['object error', withData({ error: { code: 'x' } })],
		['array message', withData({ message: ['a'] })],
		['empty strings', withData({ message: '', error: '' })],
	])('returns the fallback for %s', (_, error) => {
		expect(apiErrorText(error, 'Failed to launch agent')).toBe(
			'Failed to launch agent',
		);
	});
});
