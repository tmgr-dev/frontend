export interface Locks {
	request(name: string, callback: () => unknown): Promise<unknown>;
}

export const withLock = <T>(
	locks: Locks | null | undefined,
	name: 'consent' | 'send' | 'identity' | 'queue',
	callback: () => T,
): Promise<Awaited<T>> => {
	try {
		return (
			locks
				? locks.request(`tmgr-telemetry-${name}`, callback)
				: Promise.resolve(callback())
		) as Promise<Awaited<T>>;
	} catch (error) {
		return Promise.reject(error);
	}
};
