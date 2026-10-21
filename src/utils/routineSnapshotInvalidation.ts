import type { DomainEventHandler } from '@/utils/domainEvents';
import type { RequestCache } from '@/utils/requestCache';

export const ROUTINE_SNAPSHOTS = /^routines-snapshot/;

/** Any routine write (this window, plugins, local workspace, other windows) drops the Daily Routines snapshots (TM-703). */
export const installRoutineSnapshotInvalidation = (
	bus: { on(handler: DomainEventHandler): () => void },
	cache: Pick<RequestCache, 'invalidate'>,
) =>
	bus.on((event) => {
		if (event.type.startsWith('routine.')) cache.invalidate(ROUTINE_SNAPSHOTS);
	});
