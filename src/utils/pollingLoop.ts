/** Poll after completion, coalesce manual refresh, pause while the document is hidden. */
export function createPollingLoop(
	task: () => Promise<unknown>,
	delay: number,
	visibility: Pick<
		Document,
		'hidden' | 'addEventListener' | 'removeEventListener'
	> = document,
) {
	let active = false;
	let pending: Promise<void> | undefined;
	let timer: ReturnType<typeof setTimeout> | undefined;
	const run = (): Promise<void> => {
		clearTimeout(timer);
		if (!active || visibility.hidden) return Promise.resolve();
		if (pending) return pending;
		pending = Promise.resolve()
			.then(task)
			.then(() => undefined)
			.catch(() => undefined)
			.finally(() => {
				pending = undefined;
				if (active && !visibility.hidden)
					timer = setTimeout(() => {
						void run();
					}, delay);
			});
		return pending;
	};
	const changed = () => {
		clearTimeout(timer);
		if (!visibility.hidden) void run();
	};
	return {
		run,
		start() {
			if (active) return;
			active = true;
			visibility.addEventListener('visibilitychange', changed);
			void run();
		},
		stop() {
			active = false;
			clearTimeout(timer);
			visibility.removeEventListener('visibilitychange', changed);
		},
	};
}
