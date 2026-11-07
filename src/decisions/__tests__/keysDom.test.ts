/** @jest-environment jsdom */
import { resolveKey, type KeyAction } from '../keys';

const base = {
	optionCount: 3,
	hasRecommended: true,
	canAnswer: true,
	canDismiss: true,
};

const mount = (html: string): HTMLElement => {
	document.body.innerHTML = html;
	return document.body;
};

const press = (
	el: Element,
	init: KeyboardEventInit = {},
	onOptionKey?: () => void,
): { actions: (KeyAction | null)[]; clicks: number; picks: number } => {
	const out = { actions: [] as (KeyAction | null)[], clicks: 0, picks: 0 };
	const clicks = (): void => {
		out.clicks += 1;
	};
	el.addEventListener('click', clicks);
	const option = el.closest('.opt');
	const optionHandler = (event: Event): void => {
		if (event.target === option && (event as KeyboardEvent).key === 'Enter') {
			event.preventDefault();
			out.picks += 1;
			onOptionKey?.();
		}
	};
	option?.addEventListener('keydown', optionHandler);
	const windowHandler = (event: KeyboardEvent): void => {
		const action = resolveKey(event, base);
		out.actions.push(action);
		if (action) event.preventDefault();
	};
	window.addEventListener('keydown', windowHandler);
	const event = new KeyboardEvent('keydown', {
		key: 'Enter',
		bubbles: true,
		cancelable: true,
		...init,
	});
	const notPrevented = el.dispatchEvent(event);
	if (notPrevented && (el.tagName === 'A' || el.tagName === 'BUTTON')) {
		(el as HTMLElement).click();
	}
	window.removeEventListener('keydown', windowHandler);
	option?.removeEventListener('keydown', optionHandler);
	el.removeEventListener('click', clicks);
	return out;
};

describe('Enter in the Decisions window', () => {
	afterEach(() => {
		document.body.innerHTML = '';
	});

	it.each([
		['a link chip button', '<button class="chip" id="t">PR</button>'],
		[
			'a markdown link',
			'<div class="md"><a id="t" href="https://x.test/">x</a></div>',
		],
		[
			'a diff pane button',
			'<div class="diff"><button id="t" type="button">Show</button></div>',
		],
		['a role=button element', '<div id="t" role="button" tabindex="0">x</div>'],
		['a summary', '<details><summary id="t">x</summary></details>'],
		['a select', '<select id="t"><option>a</option></select>'],
	])('on %s never answers and keeps the native action', (_, html) => {
		mount(html);
		const el = document.getElementById('t') as HTMLElement;
		for (const shiftKey of [false, true]) {
			const out = press(el, { shiftKey });
			expect(out.actions).toEqual([null]);
			if (el.tagName === 'A' || el.tagName === 'BUTTON') {
				expect(out.clicks).toBe(1);
			}
		}
	});

	it('on a node inside a link still defers to the link', () => {
		mount('<a href="https://x.test/"><span id="t">x</span></a>');
		expect(press(document.getElementById('t') as Element).actions).toEqual([
			null,
		]);
	});

	it('on option 2 answers option 2 only', () => {
		mount(
			'<div class="opt" id="t" role="button" tabindex="0"><span>two</span></div>',
		);
		const out = press(document.getElementById('t') as Element);
		expect(out.picks).toBe(1);
		expect(out.actions).toEqual([null]);
	});

	it('on the card body still accepts the recommended option', () => {
		mount('<div id="t">card</div>');
		expect(press(document.getElementById('t') as Element).actions).toEqual([
			{ type: 'accept', withNote: false },
		]);
		expect(press(document.body, { shiftKey: true }).actions).toEqual([
			{ type: 'accept', withNote: true },
		]);
	});

	it('does nothing once another handler has taken the event', () => {
		mount('<div id="t">card</div>');
		const el = document.getElementById('t') as HTMLElement;
		const taker = (event: Event): void => event.preventDefault();
		el.addEventListener('keydown', taker);
		expect(press(el).actions).toEqual([null]);
		el.removeEventListener('keydown', taker);
	});

	it('does not treat typing as a native action', () => {
		mount('<textarea id="t"></textarea>');
		expect(press(document.getElementById('t') as Element).actions).toEqual([
			null,
		]);
	});
});
