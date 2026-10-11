import { isPlainTaskLinkClick } from '../taskLinkClicks';

const click = (extra = {}) => ({
	button: 0,
	metaKey: false,
	ctrlKey: false,
	shiftKey: false,
	altKey: false,
	defaultPrevented: false,
	...extra,
});

describe('isPlainTaskLinkClick', () => {
	it('takes a plain click on a task link', () => {
		expect(isPlainTaskLinkClick(click(), 'https://tmgr.dev/w/tasks/1')).toBe(true);
		expect(isPlainTaskLinkClick(click(), 'tmgr://task/w/1')).toBe(true);
	});

	it('leaves other links alone', () => {
		expect(isPlainTaskLinkClick(click(), 'https://tmgr.dev/w/pages/a')).toBe(false);
		expect(isPlainTaskLinkClick(click(), 'https://example.com')).toBe(false);
		expect(isPlainTaskLinkClick(click(), null)).toBe(false);
	});

	it('leaves modified, non-primary and handled clicks alone', () => {
		const href = 'https://tmgr.dev/w/tasks/1';
		for (const extra of [
			{ metaKey: true },
			{ ctrlKey: true },
			{ shiftKey: true },
			{ altKey: true },
			{ button: 1 },
			{ defaultPrevented: true },
		]) {
			expect(isPlainTaskLinkClick(click(extra), href)).toBe(false);
		}
	});
});
