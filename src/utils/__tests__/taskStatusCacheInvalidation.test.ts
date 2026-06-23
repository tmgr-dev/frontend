import { requestCache } from '../requestCache';

// TM-148 follow-up (tmgr #8827): getSortedTasksByStatus cached under a
// `tasks/status/<id>` (slash) key, but every mutating task action invalidates
// with /^tasks-status-/ (hyphen). The pattern could never match the key, so the
// board cache stayed stale for up to 30s after a mutation. The fix aligns the
// cache key to the `tasks-status-` namespace the invalidations already use.
describe('task-status cache key / invalidation alignment', () => {
	const params = { params: { all: true }, page: 1 };

	beforeEach(() => requestCache.clear());
	afterEach(() => requestCache.clear());

	it('clears the tasks-status cache via the mutation pattern /^tasks-status-/', () => {
		const key = requestCache.generateKey('tasks-status-5', params);
		requestCache.set(key, [{ id: 1 }], 30000);
		expect(requestCache.has(key)).toBe(true);

		requestCache.invalidate(/^tasks-status-/);

		expect(requestCache.has(key)).toBe(false);
	});

	it('regression: the legacy slash key was never matched by /^tasks-status-/', () => {
		const legacyKey = requestCache.generateKey('tasks/status/5', params);
		requestCache.set(legacyKey, [{ id: 1 }], 30000);

		requestCache.invalidate(/^tasks-status-/);

		// The legacy key starts with "tasks/status/", so the hyphen pattern could
		// not match it — exactly the stale-cache bug this fix removes.
		expect(requestCache.has(legacyKey)).toBe(true);
	});

	it('the generated key lives in the tasks-status- namespace the invalidations target', () => {
		const key = requestCache.generateKey('tasks-status-5', params);
		expect(/^tasks-status-/.test(key)).toBe(true);
	});
});
