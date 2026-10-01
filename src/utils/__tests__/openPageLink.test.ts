import { openPageLink, type OpenPageLinkDeps } from '../openPageLink';

const setup = (over: Partial<OpenPageLinkDeps> = {}) => {
	const deps: jest.Mocked<OpenPageLinkDeps> = {
		loadWorkspaces: jest.fn(async () => [
			{ id: 1, code: 'work' },
			{ id: -3, code: 'local-proj' },
		]),
		currentWorkspaceId: jest.fn(() => 1),
		resolvePageId: jest.fn(async () => 42),
		focusPageWindow: jest.fn(async () => false),
		showMain: jest.fn(async () => undefined),
		navigate: jest.fn(async () => undefined),
		...over,
	} as any;
	return deps;
};

describe('openPageLink', () => {
	it('navigates the main window to the page, which switches the workspace', async () => {
		const deps = setup({ currentWorkspaceId: jest.fn(() => 1) });
		expect(await openPageLink({ workspaceCode: 'local-proj', slug: 'ivan' }, deps)).toBe(true);
		expect(deps.resolvePageId).not.toHaveBeenCalled();
		expect(deps.showMain).toHaveBeenCalled();
		expect(deps.navigate).toHaveBeenCalledWith('/local-proj/pages/ivan');
	});

	it('focuses an existing page window of the current workspace without touching the main window', async () => {
		const deps = setup({ focusPageWindow: jest.fn(async () => true) });
		expect(await openPageLink({ workspaceCode: 'work', slug: 'ivan' }, deps)).toBe(true);
		expect(deps.focusPageWindow).toHaveBeenCalledWith(42, 'work');
		expect(deps.navigate).not.toHaveBeenCalled();
		expect(deps.showMain).not.toHaveBeenCalled();
	});

	it('falls back to the main window when no window is open or the page cannot be resolved', async () => {
		const deps = setup({ resolvePageId: jest.fn(async () => { throw new Error('404'); }) });
		await openPageLink({ workspaceCode: 'work', slug: 'ivan' }, deps);
		expect(deps.navigate).toHaveBeenCalledWith('/work/pages/ivan');
		const second = setup();
		await openPageLink({ workspaceCode: 'work', slug: 'ivan' }, second);
		expect(second.focusPageWindow).toHaveBeenCalled();
		expect(second.navigate).toHaveBeenCalledWith('/work/pages/ivan');
	});

	it('does nothing for an unknown workspace', async () => {
		const deps = setup();
		expect(await openPageLink({ workspaceCode: 'nope', slug: 'x' }, deps)).toBe(false);
		expect(deps.navigate).not.toHaveBeenCalled();
	});
});
