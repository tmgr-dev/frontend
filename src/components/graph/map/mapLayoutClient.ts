import {
	MapLayoutCore,
	type LayoutInit,
	type LayoutStep,
} from './mapLayoutCore';

export interface MapLayout {
	start: (init: LayoutInit) => void;
	setGroup: (group: boolean) => void;
	dispose: () => void;
}

export const createMapLayout = (
	onStep: (step: LayoutStep) => void,
): MapLayout => {
	let worker: Worker | null = null;
	if (typeof Worker !== 'undefined') {
		try {
			worker = new Worker(new URL('./mapLayout.worker.ts', import.meta.url), {
				type: 'module',
			});
		} catch {
			worker = null;
		}
	}

	let core: MapLayoutCore | null = null;
	let timer: ReturnType<typeof setTimeout> | undefined;
	let generation = 0;

	const loop = (token: number) => {
		if (!core || token !== generation) return;
		const step = core.step(10);
		onStep(step);
		if (!step.done) timer = setTimeout(() => loop(token), 0);
	};
	const local = (init: LayoutInit) => {
		clearTimeout(timer);
		core = new MapLayoutCore(init);
		loop(++generation);
	};

	if (worker) {
		worker.onmessage = (e: MessageEvent<LayoutStep>) => onStep(e.data);
		worker.onerror = () => {
			worker?.terminate();
			worker = null;
			if (lastInit) local(lastInit);
		};
	}
	let lastInit: LayoutInit | null = null;

	return {
		start(init) {
			lastInit = init;
			if (worker) worker.postMessage({ type: 'init', init });
			else local(init);
		},
		setGroup(group) {
			if (lastInit) lastInit = { ...lastInit, group };
			if (worker) worker.postMessage({ type: 'group', group });
			else if (core) {
				core.setGroup(group);
				loop(++generation);
			}
		},
		dispose() {
			generation++;
			clearTimeout(timer);
			worker?.terminate();
			worker = null;
			core = null;
		},
	};
};
