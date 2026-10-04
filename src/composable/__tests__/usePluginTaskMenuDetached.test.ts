import type { TaskMenuItem } from '@/pluginSystem/taskMenu';
import {
	TASK_MENU_ITEMS,
	TASK_MENU_REQUEST,
	installTaskMenuRelayClient,
} from '@/pluginSystem/taskMenuRelay';
import { usePluginTaskMenu } from '../usePluginTaskMenu';

jest.mock('@/components/ui/toast/use-toast', () => ({
	useToast: () => ({ toast: jest.fn() }),
}));

const mockHandlers: Record<string, (event: { payload: unknown }) => void> = {};
const mockSnapshot = { current: null as unknown };

jest.mock('@tauri-apps/api/event', () => ({
	listen: jest.fn(async (channel: string, handler: any) => {
		mockHandlers[channel] = handler;
	}),
	emit: jest.fn(),
	emitTo: jest.fn(async (_target: string, channel: string) => {
		if (channel === 'plugin-task-menu://request')
			mockHandlers['plugin-task-menu://items']?.({
				payload: mockSnapshot.current,
			});
	}),
}));

const item: TaskMenuItem = {
	pluginId: 'dev.a',
	pluginName: 'A',
	command: 'dev.a.go',
	title: 'Go',
};

beforeAll(() => {
	(globalThis as any).__TAURI_INTERNALS__ = {
		metadata: { currentWindow: { label: 'task-3' } },
	};
});

afterAll(() => {
	delete (globalThis as any).__TAURI_INTERNALS__;
});

it('uses the relay channels the main window listens on', () => {
	expect(TASK_MENU_REQUEST).toBe('plugin-task-menu://request');
	expect(TASK_MENU_ITEMS).toBe('plugin-task-menu://items');
});

it('shows items that arrive after the menu was first rendered', async () => {
	const { items } = usePluginTaskMenu();
	expect(items.value).toEqual([]);
	mockSnapshot.current = { items: [item], workspaceId: 5 };
	await installTaskMenuRelayClient('task-3', () => 5);
	expect(items.value).toEqual([item]);
});

it('hides items when the window shows another workspace', async () => {
	const { items } = usePluginTaskMenu();
	mockSnapshot.current = { items: [item], workspaceId: 5 };
	await installTaskMenuRelayClient('task-3', () => 6);
	expect(items.value).toEqual([]);
});
