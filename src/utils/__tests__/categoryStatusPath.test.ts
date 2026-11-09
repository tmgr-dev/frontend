import { categoryStatusPath } from '../url';

describe('categoryStatusPath', () => {
	it('builds the workspace path for a category status', () => {
		expect(categoryStatusPath('demo', 5, 'open')).toBe(
			'/demo/categories/5/children/open',
		);
	});

	it('returns null on the top-level list where no route takes a status', () => {
		expect(categoryStatusPath('demo', undefined, 'open')).toBeNull();
	});

	it('falls back to the legacy redirect path without a workspace', () => {
		expect(categoryStatusPath(null, 5, 'done')).toBe(
			'/projects-categories/5/children/done',
		);
	});
});
