import { installTaskWindowNavigationGuard } from '../taskWindowBridge';

describe('installTaskWindowNavigationGuard', () => {
	const setup = () => {
		let guard!: (to: { name?: unknown; fullPath: string }) => boolean | void;
		const router = { beforeEach: (g: typeof guard) => ((guard = g), () => {}) };
		const send = jest.fn();
		installTaskWindowNavigationGuard(router, send);
		return { guard, send };
	};

	it('lets the task window route through', () => {
		const { guard, send } = setup();
		expect(guard({ name: 'TaskWindow', fullPath: '/a/task-window/1' })).toBeUndefined();
		expect(send).not.toHaveBeenCalled();
	});

	it('cancels any other navigation and hands the path to the main window', () => {
		const { guard, send } = setup();
		expect(guard({ name: 'Settings', fullPath: '/settings' })).toBe(false);
		expect(send).toHaveBeenCalledWith('/settings');
	});
});

describe('installTaskWindowNavigationGuard for page windows', () => {
	it('lets only the named route through', () => {
		let guard!: (to: { name?: unknown; fullPath: string }) => boolean | void;
		const router = { beforeEach: (g: typeof guard) => ((guard = g), () => {}) };
		const send = jest.fn();
		installTaskWindowNavigationGuard(router, send, 'PageWindow');
		expect(guard({ name: 'PageWindow', fullPath: '/a/page-window/x' })).toBeUndefined();
		expect(guard({ name: 'TaskWindow', fullPath: '/a/task-window/1' })).toBe(false);
		expect(send).toHaveBeenCalledWith('/a/task-window/1');
	});
});
