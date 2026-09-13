/** Coalesce edits that arrive during serialization; never overlap editor operations. */
export function createSerialEditor<T>(
	save: () => Promise<T>,
	publish: (value: T) => void,
) {
	let dirty = false;
	let pending: Promise<T | undefined> | undefined;
	let latest: T | undefined;
	const flush = (): Promise<T | undefined> => {
		if (pending) return pending;
		pending = (async () => {
			while (dirty) {
				dirty = false;
				latest = await save();
				publish(latest);
			}
			return latest;
		})().finally(() => {
			pending = undefined;
		});
		return pending;
	};
	return {
		changed() {
			dirty = true;
			return flush();
		},
		flush,
		hasPending: () => dirty || !!pending,
	};
}

/** One queue for editor reads, document replacement and teardown. */
export function createEditorOperationQueue() {
	let tail: Promise<unknown> = Promise.resolve();
	return <T>(operation: () => Promise<T> | T): Promise<T> => {
		const result = tail.then(operation);
		tail = result.catch(() => undefined);
		return result;
	};
}
