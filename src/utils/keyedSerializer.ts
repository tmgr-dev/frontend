// Serialises async tasks per key: tasks sharing a key run strictly in
// submission order (never overlapping); tasks with different keys run
// independently. Used to serialise stateful toggle requests per (comment,emoji)
// so the server processes them in click order and the last response is
// authoritative.
export type KeyedSerializer = (
	key: string,
	task: () => Promise<unknown>,
) => Promise<unknown>;

export const createKeyedSerializer = (): KeyedSerializer => {
	const chains = new Map<string, Promise<unknown>>();

	return (key, task) => {
		const pending = chains.get(key) ?? Promise.resolve();
		const chained = pending.catch(() => undefined).then(() => task());
		chains.set(key, chained);
		const cleanup = () => {
			if (chains.get(key) === chained) {
				chains.delete(key);
			}
		};
		// Attach as both handlers: runs cleanup on settle AND marks `chained`
		// handled so a fire-and-forget caller never triggers unhandledRejection.
		chained.then(cleanup, cleanup);
		return chained;
	};
};
