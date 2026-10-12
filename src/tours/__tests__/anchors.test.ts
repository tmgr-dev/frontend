/** @jest-environment jsdom */
import { anchorSelector, isOccluded, isVisible, pickAnchorName, resolveAnchor } from '../anchors';
import type { StepDef } from '../types';

const step = (extra: Partial<StepDef> = {}): StepDef => ({
	id: 's',
	type: 'spot',
	anchor: 'nav.sidebar',
	title: 't',
	body: 'b',
	...extra,
});

const box = (el: Element, w = 10, h = 10) => {
	(el as HTMLElement).getBoundingClientRect = () =>
		({ width: w, height: h, top: 0, left: 0, right: w, bottom: h, x: 0, y: 0 }) as DOMRect;
};

const add = (html: string) => {
	const host = document.createElement('div');
	host.innerHTML = html;
	document.body.appendChild(host);
	host.querySelectorAll('*').forEach((el) => box(el));
	return host;
};

afterEach(() => {
	document.body.innerHTML = '';
	jest.useRealTimers();
});

describe('isVisible', () => {
	it('needs a non-zero box', () => {
		const host = add('<i data-tour="a"></i>');
		const el = host.firstElementChild!;
		expect(isVisible(el)).toBe(true);
		box(el, 0, 0);
		expect(isVisible(el)).toBe(false);
	});

	it('rejects display:none and visibility:hidden', () => {
		const host = add('<i data-tour="a" style="display:none"></i><b data-tour="b" style="visibility:hidden"></b>');
		expect(isVisible(host.children[0])).toBe(false);
		expect(isVisible(host.children[1])).toBe(false);
	});
});

describe('isOccluded', () => {
	const stubHit = (hit: (x: number, y: number) => Element | null) => {
		(document as unknown as { elementFromPoint: unknown }).elementFromPoint = hit;
	};
	afterEach(() => {
		delete (document as unknown as { elementFromPoint?: unknown }).elementFromPoint;
	});

	it('is false when the browser cannot hit-test', () => {
		const host = add('<i data-tour="a"></i>');
		expect(isOccluded(host.firstElementChild!)).toBe(false);
	});

	it('is false when the element or a relative of it is on top at every sample point', () => {
		const host = add('<b id="p"><i data-tour="a"></i></b>');
		const el = host.querySelector('i')!;
		stubHit(() => el);
		expect(isOccluded(el)).toBe(false);
		stubHit(() => host.querySelector('b'));
		expect(isOccluded(el)).toBe(false);
	});

	it('is true when another element covers a sample point', () => {
		const host = add('<i data-tour="a"></i><u id="panel"></u>');
		const el = host.querySelector('i')!;
		const panel = host.querySelector('u')!;
		stubHit((x) => (x > 5 ? panel : el));
		expect(isOccluded(el)).toBe(true);
	});

	it('ignores the sidebar resize rail that overlaps the sidebar edge', () => {
		const host = add('<i data-tour="a"></i><button data-sidebar="rail"></button>');
		const el = host.querySelector('i')!;
		stubHit(() => host.querySelector('button'));
		expect(isOccluded(el)).toBe(false);
	});

	it('ignores the tour own overlay layers', () => {
		const host = add('<i data-tour="a"></i><div class="tour-backdrop"></div>');
		const el = host.querySelector('i')!;
		stubHit(() => host.querySelector('.tour-backdrop'));
		expect(isOccluded(el)).toBe(false);
	});
});

describe('pickAnchorName', () => {
	it('uses mobileAnchor under 640px only', () => {
		const s = step({ mobileAnchor: 'nav.menu-button' });
		expect(pickAnchorName(s, true)).toBe('nav.menu-button');
		expect(pickAnchorName(s, false)).toBe('nav.sidebar');
		expect(pickAnchorName(step(), true)).toBe('nav.sidebar');
		expect(pickAnchorName(step({ anchor: undefined }), false)).toBeUndefined();
	});
});

describe('resolveAnchor occlusion', () => {
	afterEach(() => {
		delete (document as unknown as { elementFromPoint?: unknown }).elementFromPoint;
	});

	it('skips anchors that sit behind another surface', async () => {
		const host = add('<i data-tour="nav.sidebar"></i><u></u>');
		const panel = host.querySelector('u')!;
		(document as unknown as { elementFromPoint: unknown }).elementFromPoint = () => panel;
		expect(await resolveAnchor(step(), { mobile: false, timeoutMs: 20 })).toBeNull();
	});
});

describe('resolveAnchor throttling', () => {
	afterEach(() => {
		delete (document as unknown as { elementFromPoint?: unknown }).elementFromPoint;
	});

	it('checks at most once per frame during a burst of mutations and samples few points', async () => {
		const host = add('<i data-tour="nav.sidebar"></i><u></u>');
		const panel = host.querySelector('u')!;
		const hit = jest.fn(() => panel);
		(document as unknown as { elementFromPoint: unknown }).elementFromPoint = hit;
		const promise = resolveAnchor(step(), { mobile: false, timeoutMs: 120 });
		for (let i = 0; i < 20; i++) {
			add('<span></span>');
			await Promise.resolve();
		}
		await promise;
		expect(hit.mock.calls.length).toBeLessThanOrEqual(4);
	});
});

describe('anchorSelector', () => {
	it('scopes to the event task', () => {
		expect(anchorSelector('list.row', 5)).toContain('[data-task-id="5"]');
		expect(anchorSelector('list.row', null)).toBe('[data-tour="list.row"]');
	});
});

describe('resolveAnchor', () => {
	it('returns the first visible match', async () => {
		const host = add('<i data-tour="nav.sidebar"></i><b data-tour="nav.sidebar" id="two"></b>');
		box(host.children[0], 0, 0);
		const el = await resolveAnchor(step(), { mobile: false });
		expect(el?.id).toBe('two');
	});

	it('waits for a late anchor', async () => {
		const promise = resolveAnchor(step(), { mobile: false, timeoutMs: 1000 });
		setTimeout(() => add('<i data-tour="nav.sidebar" id="late"></i>'), 30);
		expect((await promise)?.id).toBe('late');
	});

	it('gives up after the timeout', async () => {
		expect(await resolveAnchor(step(), { mobile: false, timeoutMs: 50 })).toBeNull();
	});

	it('returns null for a step without an anchor', async () => {
		expect(await resolveAnchor(step({ anchor: undefined, type: 'modal' }), { mobile: false })).toBeNull();
	});

	it('narrows to the task of the last task.created for anchorScope eventTask', async () => {
		add('<div data-tour="list.row" data-task-id="1" id="a"></div><div data-tour="list.row" data-task-id="2" id="b"></div>');
		const el = await resolveAnchor(step({ anchor: 'list.row', anchorScope: 'eventTask' }), { mobile: false, eventTaskId: 2 });
		expect(el?.id).toBe('b');
	});

	it('uses the mobile anchor on small screens', async () => {
		add('<i data-tour="nav.menu-button" id="m"></i>');
		const el = await resolveAnchor(step({ mobileAnchor: 'nav.menu-button' }), { mobile: true });
		expect(el?.id).toBe('m');
	});

	it('can be cancelled', async () => {
		const ctl = new AbortController();
		const promise = resolveAnchor(step(), { mobile: false, timeoutMs: 5000, signal: ctl.signal });
		ctl.abort();
		expect(await promise).toBeNull();
	});
});
