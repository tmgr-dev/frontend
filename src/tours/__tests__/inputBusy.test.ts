/**
 * @jest-environment jsdom
 */
import { isInputBusy } from '../boot';

describe('isInputBusy', () => {
	afterEach(() => {
		document.body.innerHTML = '';
	});

	it('is idle on a plain page', () => {
		document.body.innerHTML = '<div class="card">A</div>';
		expect(isInputBusy()).toBe(false);
	});

	it('is busy while a Sortable drag holds an element', () => {
		document.body.innerHTML = '<div class="card sortable-chosen">A</div>';
		expect(isInputBusy()).toBe(true);
	});

	it('is busy with an open dialog or a focused input', () => {
		document.body.innerHTML = '<div role="dialog" data-state="open"></div>';
		expect(isInputBusy()).toBe(true);
		document.body.innerHTML = '<input id="i" />';
		(document.getElementById('i') as HTMLInputElement).focus();
		expect(isInputBusy()).toBe(true);
	});
});
