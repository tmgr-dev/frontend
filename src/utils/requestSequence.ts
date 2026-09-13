/** Prevent late asynchronous results from updating a newer or disposed view. */
export function createRequestSequence() {
	let version = 0;
	let disposed = false;
	return {
		begin: () => ++version,
		isCurrent: (request: number) => !disposed && request === version,
		dispose: () => {
			disposed = true;
			version++;
		},
	};
}
