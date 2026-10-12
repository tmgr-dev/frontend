import { clampOffset } from '../overlay/clamp';

const vp = { width: 1000, height: 800 };
const rect = (left: number, top: number, width: number, height: number) => ({
	left,
	top,
	right: left + width,
	bottom: top + height,
	width,
	height,
});

describe('clampOffset', () => {
	it('is zero when the rect already fits with padding', () => {
		expect(clampOffset(rect(100, 100, 300, 200), vp, 8)).toEqual({ dx: 0, dy: 0 });
	});

	it('pulls a rect hanging off the right and bottom back in', () => {
		expect(clampOffset(rect(800, 700, 400, 300), vp, 8)).toEqual({ dx: -208 - 0, dy: -208 + 0 });
	});

	it('pushes a rect off the left and top back in', () => {
		expect(clampOffset(rect(-50, -20, 300, 200), vp, 8)).toEqual({ dx: 58, dy: 28 });
	});

	it('aligns to the top-left when the rect is bigger than the viewport', () => {
		expect(clampOffset(rect(300, 300, 1200, 1000), vp, 8)).toEqual({ dx: -292, dy: -292 });
	});
});

import { clampIntoViewport, createFrameClamp } from '../overlay/clamp';

const stubEl = (classes: string[], r: ReturnType<typeof rect>) => {
	const el: any = {
		style: { translate: 'stale' },
		classList: { contains: (c: string) => classes.includes(c) },
		getBoundingClientRect: () => r,
	};
	return el as HTMLElement;
};

describe('clampIntoViewport', () => {
	beforeEach(() => {
		(global as any).window = { innerWidth: 1000, innerHeight: 800 };
	});
	afterEach(() => {
		delete (global as any).window;
	});

	it('translates an overflowing element', () => {
		const el = stubEl([], rect(900, 100, 300, 100));
		clampIntoViewport(el);
		expect(el.style.translate).toBe('-208px 0px');
	});

	it('leaves a bottom sheet untouched and resets any translate', () => {
		const el = stubEl(['tour-host--sheet'], rect(900, 100, 300, 100));
		clampIntoViewport(el);
		expect(el.style.translate).toBe('');
	});
});

describe('createFrameClamp', () => {
	let frames: Array<() => void>;
	let listeners: Record<string, Array<() => void>>;
	beforeEach(() => {
		frames = [];
		listeners = {};
		(global as any).requestAnimationFrame = (fn: () => void) => frames.push(fn);
		(global as any).cancelAnimationFrame = (id: number) => {
			frames[id - 1] = () => {};
		};
		(global as any).window = {
			innerWidth: 1000,
			innerHeight: 800,
			addEventListener: (t: string, fn: () => void) => (listeners[t] = [...(listeners[t] ?? []), fn]),
			removeEventListener: (t: string, fn: () => void) => (listeners[t] = (listeners[t] ?? []).filter((f) => f !== fn)),
		};
	});
	afterEach(() => {
		delete (global as any).window;
		delete (global as any).requestAnimationFrame;
		delete (global as any).cancelAnimationFrame;
	});

	it('clamps in the next frame, not synchronously', () => {
		const el = stubEl([], rect(900, 100, 300, 100));
		const c = createFrameClamp(() => el);
		c.start();
		expect(el.style.translate).toBe('stale');
		frames.forEach((f) => f());
		expect(el.style.translate).toBe('-208px 0px');
	});

	it('re-clamps on scroll and resize, and stops listening after stop', () => {
		const el = stubEl([], rect(900, 100, 300, 100));
		const c = createFrameClamp(() => el);
		c.start();
		expect(listeners.scroll).toHaveLength(1);
		expect(listeners.resize).toHaveLength(1);
		c.stop();
		expect(listeners.scroll).toHaveLength(0);
		expect(listeners.resize).toHaveLength(0);
	});

	it('skips the driver sheet mode', () => {
		const el = stubEl(['tour-sheet-mode'], rect(900, 100, 300, 100));
		const c = createFrameClamp(() => el);
		c.start();
		frames.forEach((f) => f());
		expect(el.style.translate).toBe('stale');
	});
});
