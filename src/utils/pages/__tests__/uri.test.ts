import { PAGE_ALLOWED_URI_REGEXP } from '../uri';

describe('PAGE_ALLOWED_URI_REGEXP', () => {
	it.each([
		'tmgr://page/1',
		'TMGR://task/2',
		'https://x.io',
		'mailto:a@b.c',
		'/rel',
		'#anchor',
	])('allows %s', (href) =>
		expect(PAGE_ALLOWED_URI_REGEXP.test(href)).toBe(true),
	);

	it.each(['javascript:alert(1)', 'vbscript:x', 'data:text/html,<b>'])(
		'blocks %s',
		(href) => expect(PAGE_ALLOWED_URI_REGEXP.test(href)).toBe(false),
	);
});
