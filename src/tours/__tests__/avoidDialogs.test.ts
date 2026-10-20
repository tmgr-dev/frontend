/**
 * @jest-environment jsdom
 */
import { visibleAvoidRects } from '../overlay/avoid';

const box = (el: HTMLElement, left: number, top: number) => {
	el.getBoundingClientRect = () =>
		({
			left,
			top,
			right: left + 100,
			bottom: top + 50,
			width: 100,
			height: 50,
		} as DOMRect);
	return el;
};

const add = (html: string) => {
	const wrap = document.createElement('div');
	wrap.innerHTML = html;
	document.body.appendChild(wrap);
	return wrap;
};

afterEach(() => {
	document.body.innerHTML = '';
});

describe('visibleAvoidRects', () => {
	it('avoids an open Modal.vue card and a radix dialog, not the overlay backdrop', () => {
		const modal = add(
			'<div data-name="overlay"><div class="backdrop"></div><div class="card"></div></div>',
		);
		box(modal.querySelector<HTMLElement>('.backdrop')!, 0, 0);
		box(modal.querySelector<HTMLElement>('.card')!, 400, 300);
		const radix = add('<div role="dialog"></div>');
		box(radix.firstElementChild as HTMLElement, 10, 20);

		expect(visibleAvoidRects().map((r) => [r.left, r.top])).toEqual([
			[400, 300],
			[10, 20],
		]);
	});

	it('never avoids the tour card itself', () => {
		const host = add(
			'<div data-tour-host="passive"><div role="dialog"></div></div>',
		);
		box(host.querySelector<HTMLElement>('[role="dialog"]')!, 0, 0);
		expect(visibleAvoidRects()).toEqual([]);
	});

	it('keeps a dialog that holds the step anchor out of the avoid list', () => {
		const wrap = add('<div role="dialog"><button id="a"></button></div>');
		box(wrap.firstElementChild as HTMLElement, 0, 0);
		expect(visibleAvoidRects(wrap.querySelector('#a'))).toEqual([]);
	});
});
