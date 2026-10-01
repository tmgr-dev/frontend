import { isDetachedWindowLabel } from '../taskWindow';
import {
	focusPageWindow,
	isInPageWindow,
	isPageWindowLabel,
	openPageWindow,
	pageWindowTarget,
	setPageWindowTitle,
} from '../pageWindow';

const invoke = jest.fn();
const setTitle = jest.fn();
jest.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
jest.mock('@tauri-apps/api/window', () => ({
	getCurrentWindow: () => ({ setTitle: (t: string) => setTitle(t) }),
}));

beforeEach(() => {
	invoke.mockReset();
	setTitle.mockReset();
});

describe('window labels', () => {
	it('matches only page windows', () => {
		expect(isPageWindowLabel('page-work-12')).toBe(true);
		expect(isPageWindowLabel('task-work-12')).toBe(false);
		expect(isPageWindowLabel('main')).toBe(false);
		expect(isPageWindowLabel(null)).toBe(false);
	});

	it('treats task and page windows as detached, nothing else', () => {
		expect(isDetachedWindowLabel('task-work-1')).toBe(true);
		expect(isDetachedWindowLabel('page-work-1')).toBe(true);
		expect(isDetachedWindowLabel('main')).toBe(false);
		expect(isDetachedWindowLabel('quick-add')).toBe(false);
		expect(isDetachedWindowLabel(null)).toBe(false);
	});

	it('detects the current window from the Tauri metadata', () => {
		expect(isInPageWindow()).toBe(false);
		(globalThis as any).__TAURI_INTERNALS__ = { metadata: { currentWindow: { label: 'page-a-1' } } };
		expect(isInPageWindow()).toBe(true);
		delete (globalThis as any).__TAURI_INTERNALS__;
	});
});

describe('pageWindowTarget', () => {
	it('builds a target with a trimmed title', () => {
		expect(pageWindowTarget({ id: 7, slug: 'ivan', title: ' Ivan ' }, 'work')).toEqual({
			pageId: 7,
			workspaceCode: 'work',
			slug: 'ivan',
			title: 'Ivan',
		});
	});

	it('gives a null title for blank titles', () => {
		expect(pageWindowTarget({ id: 7, slug: 'x', title: '  ' }, 'work')?.title).toBeNull();
	});

	it('is null without an id, slug or workspace code', () => {
		expect(pageWindowTarget({ slug: 'x' }, 'work')).toBeNull();
		expect(pageWindowTarget({ id: 0, slug: 'x' }, 'work')).toBeNull();
		expect(pageWindowTarget({ id: 1 }, 'work')).toBeNull();
		expect(pageWindowTarget({ id: 1, slug: 'x' }, null)).toBeNull();
		expect(pageWindowTarget(null, 'work')).toBeNull();
	});
});

describe('commands', () => {
	it('opens and focuses through the Rust commands', async () => {
		invoke.mockResolvedValue(true);
		const target = { pageId: 7, workspaceCode: 'work', slug: 'ivan', title: 'Ivan' };
		await openPageWindow(target);
		expect(invoke).toHaveBeenCalledWith('open_page_window', target);
		expect(await focusPageWindow(7, 'work')).toBe(true);
		expect(invoke).toHaveBeenCalledWith('focus_page_window', { pageId: 7, workspaceCode: 'work' });
	});

	it('sets a single-line window title', async () => {
		await setPageWindowTitle('A\n  B');
		expect(setTitle).toHaveBeenCalledWith('A B');
	});
});
