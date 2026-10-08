import { MapLayoutCore, type LayoutInit } from './mapLayoutCore';

type In =
	| { type: 'init'; init: LayoutInit }
	| { type: 'group'; group: boolean };

let core: MapLayoutCore | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
const scope = self as unknown as {
	onmessage: ((e: MessageEvent<In>) => void) | null;
	postMessage: (message: unknown, transfer: Transferable[]) => void;
};

const loop = () => {
	clearTimeout(timer);
	if (!core) return;
	const step = core.step(12);
	scope.postMessage({ type: 'positions', ...step }, [step.positions.buffer]);
	if (!step.done) timer = setTimeout(loop, 0);
};

scope.onmessage = (e) => {
	if (e.data.type === 'init') core = new MapLayoutCore(e.data.init);
	else core?.setGroup(e.data.group);
	loop();
};
