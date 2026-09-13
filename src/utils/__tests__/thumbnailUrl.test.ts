import { withThumbParam } from '../thumbnailUrl';

describe('withThumbParam', () => {
	it('adds the parameter to a bare content URL', () => {
		expect(withThumbParam('https://api.tmgr.dev/api/files/7/content')).toBe(
			'https://api.tmgr.dev/api/files/7/content?thumb=true',
		);
	});

	it('appends to a signed link without disturbing its signature', () => {
		// The signature covers the file, the user and the deadline - not the query as a whole -
		// so asking the same link for a smaller rendering is legitimate.
		const signed =
			'https://api.tmgr.dev/api/files/7/content?uid=74&exp=1789&sig=abc';

		expect(withThumbParam(signed)).toBe(`${signed}&thumb=true`);
	});

	it('leaves a URL that already asks for a thumbnail alone', () => {
		const url = 'https://api.tmgr.dev/api/files/7/content?thumb=true';

		expect(withThumbParam(url)).toBe(url);
	});

	it('does not touch a blob URL, which has no query to speak of', () => {
		expect(withThumbParam('blob:https://tmgr.dev/abc-123')).toBe(
			'blob:https://tmgr.dev/abc-123',
		);
	});
});
