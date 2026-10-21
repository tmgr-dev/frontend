import { createDomainEvents } from '../domainEvents';
import { RequestCache } from '../requestCache';
import { installRoutineSnapshotInvalidation } from '../routineSnapshotInvalidation';

describe('installRoutineSnapshotInvalidation', () => {
	it('drops routine snapshots on every routine event', () => {
		const bus = createDomainEvents();
		const invalidate = jest.fn();
		installRoutineSnapshotInvalidation(bus, { invalidate });

		bus.emit({
			type: 'routine.created',
			workspaceId: 1,
			routineId: 2,
			routine: {},
		});
		bus.emit({ type: 'routine.updated', workspaceId: 1, routineId: 2 });
		bus.emit({ type: 'routine.deleted', workspaceId: 1, routineId: 2 });

		expect(invalidate).toHaveBeenCalledTimes(3);
		const pattern = invalidate.mock.calls[0][0] as RegExp;
		expect(pattern.test('routines-snapshot{"workspace":1}')).toBe(true);
		expect(pattern.test('tasks-status-list-snapshot')).toBe(false);
	});

	it('ignores task events and stops after unsubscribe', () => {
		const bus = createDomainEvents();
		const invalidate = jest.fn();
		const off = installRoutineSnapshotInvalidation(bus, { invalidate });

		bus.emit({ type: 'task.updated', workspaceId: 1, taskId: 2, task: {} });
		off();
		bus.emit({ type: 'routine.deleted', workspaceId: 1, routineId: 2 });

		expect(invalidate).not.toHaveBeenCalled();
	});

	it('removes only routine snapshots from the cache', () => {
		const bus = createDomainEvents();
		const cache = new RequestCache();
		cache.setContext('7:1');
		const routines = cache.generateKey('routines-snapshot', {
			workspace: 1,
			from: '2026-10-21',
			to: '2026-10-21',
		});
		cache.set(routines, { entries: [] });
		cache.set('tasks-status-list-snapshot', { tasks: [] });
		installRoutineSnapshotInvalidation(bus, cache);

		bus.emit({ type: 'routine.deleted', workspaceId: 1, routineId: 2 });

		expect(cache.get(routines)).toBeNull();
		expect(cache.get('tasks-status-list-snapshot')).toEqual({ tasks: [] });
	});
});
