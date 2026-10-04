import { pluginState, setPluginHost } from '@/pluginSystem/state';
import type { TaskMenuItem } from '@/pluginSystem/taskMenu';
import { currentTaskMenuItems } from '@/pluginSystem/taskMenuItems';
import { nextTick, watch } from 'vue';
import { usePluginTaskMenu } from '../usePluginTaskMenu';

const mockToast = jest.fn();
jest.mock('@/components/ui/toast/use-toast', () => ({
	useToast: () => ({ toast: mockToast }),
}));

const item = (pluginId: string, command: string): TaskMenuItem => ({
	pluginId,
	pluginName: pluginId,
	command,
	title: command,
});

const fakeHost = (
	items: () => TaskMenuItem[],
	runTaskMenuCommand = jest.fn(),
) => ({ taskMenuItems: items, runTaskMenuCommand } as any);

const resetState = () => {
	setPluginHost(null);
	pluginState.revision = 0;
	pluginState.revisions = {};
	pluginState.plugins = {};
	mockToast.mockClear();
};

beforeEach(resetState);

describe('usePluginTaskMenu', () => {
	it('has no items without a host', () => {
		const { items, useSubmenu } = usePluginTaskMenu();
		expect(items.value).toEqual([]);
		expect(useSubmenu.value).toBe(false);
	});

	it('lists the host items and re-evaluates when plugins start or re-register', async () => {
		let current: TaskMenuItem[] = [];
		setPluginHost(fakeHost(() => current));
		const { items } = usePluginTaskMenu();
		expect(items.value).toEqual([]);

		current = [item('a', 'one')];
		pluginState.revision++;
		expect(items.value).toEqual(current);

		current = [item('a', 'one'), item('a', 'two')];
		pluginState.revisions = { a: 1 };
		expect(items.value).toHaveLength(2);

		current = [];
		pluginState.plugins = { a: { status: 'stopped' } } as any;
		expect(items.value).toEqual([]);
	});

	it('switches to a submenu above four items', () => {
		setPluginHost(
			fakeHost(() => ['1', '2', '3', '4', '5'].map((c) => item('a', c))),
		);
		expect(usePluginTaskMenu().useSubmenu.value).toBe(true);
		setPluginHost(
			fakeHost(() => ['1', '2', '3', '4'].map((c) => item('a', c))),
		);
		pluginState.revision++;
		expect(usePluginTaskMenu().useSubmenu.value).toBe(false);
	});

	it('runs the command of the clicked item with the task id', async () => {
		const run = jest.fn().mockResolvedValue(undefined);
		setPluginHost(fakeHost(() => [], run));
		await usePluginTaskMenu().run(item('dev.a', 'dev.a.open'), 42);
		expect(run).toHaveBeenCalledWith('dev.a', 'dev.a.open', 42);
		expect(mockToast).not.toHaveBeenCalled();
	});

	it('toasts the error message when the command fails', async () => {
		setPluginHost(
			fakeHost(() => [], jest.fn().mockRejectedValue(new Error('boom'))),
		);
		await usePluginTaskMenu().run(item('dev.a', 'x'), 1);
		expect(mockToast).toHaveBeenCalledWith({
			title: 'The plugin command failed',
			description: 'boom',
			variant: 'destructive',
		});
	});

	it('stays silent when the plugin crashed', async () => {
		pluginState.plugins = { 'dev.a': { status: 'crashed' } } as any;
		setPluginHost(
			fakeHost(() => [], jest.fn().mockRejectedValue(new Error('boom'))),
		);
		await usePluginTaskMenu().run(item('dev.a', 'x'), 1);
		expect(mockToast).not.toHaveBeenCalled();
	});
});

describe('currentTaskMenuItems', () => {
	it('is reactive to plugin status changes', async () => {
		let current = [item('a', 'one')];
		setPluginHost(fakeHost(() => current));
		pluginState.plugins = { a: { status: 'running' } } as any;
		const seen: number[] = [];
		watch(
			() => currentTaskMenuItems().length,
			(n) => seen.push(n),
		);
		current = [];
		pluginState.plugins.a.status = 'stopped';
		await nextTick();
		expect(seen).toEqual([0]);
	});
});
