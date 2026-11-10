export interface LockManagerLike {
	request(
		name: string,
		options: { signal?: AbortSignal },
		callback: () => Promise<void>,
	): Promise<unknown>;
}

/** One tab per name runs the plugins; the others queue and take over when it closes or switches away. */
export const createLeader = (
	locks: LockManagerLike | undefined | null = typeof navigator === 'undefined'
		? null
		: (navigator as { locks?: LockManagerLike }).locks,
) => {
	let abort: AbortController | null = null;
	let release: (() => void) | null = null;

	const leave = () => {
		abort?.abort();
		abort = null;
		release?.();
		release = null;
	};

	/** Resolves true once this tab leads `name`; false when it was superseded by another `lead` or `leave` call. */
	const lead = async (name: string): Promise<boolean> => {
		leave();
		if (!locks) return true;
		const mine = new AbortController();
		abort = mine;
		return new Promise<boolean>((resolve) => {
			locks
				.request(name, { signal: mine.signal }, () => {
					if (mine.signal.aborted) {
						resolve(false);
						return Promise.resolve();
					}
					resolve(true);
					return new Promise<void>((done) => {
						release = done;
					});
				})
				.catch(() => resolve(false));
		});
	};

	return { lead, leave };
};
