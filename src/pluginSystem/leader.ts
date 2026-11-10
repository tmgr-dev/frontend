export interface LockManagerLike {
	request(
		name: string,
		options: { signal?: AbortSignal; ifAvailable?: boolean },
		callback: (lock: unknown) => Promise<void>,
	): Promise<unknown>;
}

/** One tab per name leads; the others queue and are promoted when it closes or switches away. */
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

	const hold = () =>
		new Promise<void>((done) => {
			release = done;
		});

	/** Resolves true when this tab leads `name` right now; otherwise false, and `onPromoted` runs if it later takes over. */
	const lead = async (
		name: string,
		onPromoted: () => void,
	): Promise<boolean> => {
		leave();
		if (!locks) return true;
		const mine = new AbortController();
		abort = mine;
		const now = await new Promise<boolean>((resolve) => {
			locks
				.request(name, { ifAvailable: true }, (lock) => {
					if (!lock || mine.signal.aborted) {
						resolve(false);
						return Promise.resolve();
					}
					resolve(true);
					return hold();
				})
				.catch(() => resolve(false));
		});
		if (now) return true;
		if (mine.signal.aborted) return false;
		locks
			.request(name, { signal: mine.signal }, () => {
				if (mine.signal.aborted) return Promise.resolve();
				onPromoted();
				return hold();
			})
			.catch(() => undefined);
		return false;
	};

	return { lead, leave };
};
