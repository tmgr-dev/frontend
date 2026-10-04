type Versioned = object | null | undefined;

const versionOf = (task: Versioned): number => {
	const parsed = Date.parse(
		(task as { updated_at?: string | null } | null)?.updated_at ?? '',
	);
	return Number.isNaN(parsed) ? 0 : parsed;
};

export const createTaskVersionGuard = (notify: (task: any) => void) => {
	let known = 0;
	let pending = 0;
	let deferred: any[] = [];

	const recordKnown = (task: Versioned) => {
		known = Math.max(known, versionOf(task));
	};

	const evaluate = (task: any) => {
		const version = versionOf(task);
		if (version && version <= known) return;
		notify(task);
	};

	const receive = (task: any) => {
		if (pending > 0) {
			deferred.push(task);
			return;
		}
		evaluate(task);
	};

	const track = async <T>(write: () => Promise<T>): Promise<T> => {
		pending++;
		try {
			const result = await write();
			recordKnown(result as Versioned);
			return result;
		} finally {
			pending--;
			if (pending === 0) {
				const queued = deferred;
				deferred = [];
				queued.forEach(evaluate);
			}
		}
	};

	return { recordKnown, receive, track };
};
