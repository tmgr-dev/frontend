/** @jest-environment jsdom */
import {
	buttonWithText,
	byTestId,
	flush,
	loadSfc,
	mountComponent,
} from './harness';

const mockPreview = jest.fn();
const mockStart = jest.fn();
const mockCurrent = jest.fn();
const mockCancel = jest.fn();
const mockCategories = jest.fn();
const mockSubscribe = jest.fn();
const mockUnsubscribe = jest.fn();

jest.mock('vuex', () => ({
	useStore: () => ({ state: { user: { id: 7 } } }),
}));
jest.mock('@/composable/usePusher', () => ({
	usePusher: () => ({
		subscribeToUser: (...args: unknown[]) => mockSubscribe(...args),
		unsubscribeHandler: (...args: unknown[]) => mockUnsubscribe(...args),
	}),
}));
jest.mock('@/actions/tmgr/categories', () => ({
	getCategoriesOfWorkspace: (...args: unknown[]) => mockCategories(...args),
}));
jest.mock('@/plugins/axios', () => ({ __esModule: true, default: {} }));
jest.mock('@/actions/tmgr/memory', () => ({
	...jest.requireActual('@/actions/tmgr/memory'),
	previewSummarize: (...args: unknown[]) => mockPreview(...args),
	startSummarize: (...args: unknown[]) => mockStart(...args),
	getCurrentSummarizeRun: (...args: unknown[]) => mockCurrent(...args),
	cancelSummarizeRun: (...args: unknown[]) => mockCancel(...args),
}));
jest.mock('@/components/Modal.vue', () => {
	const { h } = jest.requireActual('vue');
	return {
		__esModule: true,
		default: {
			render(this: any) {
				return h('div', null, this.$slots['modal-body']?.());
			},
		},
	};
});
jest.mock('@/components/ui/button', () => {
	const { h } = jest.requireActual('vue');
	return {
		Button: {
			render(this: any) {
				return h('button', null, this.$slots.default?.());
			},
		},
	};
});

const topics = [
	{ key: 'people', title: 'People' },
	{ key: 'decisions', title: 'Decisions' },
];

const preview = (extra: Record<string, unknown> = {}) => ({
	tasks: 124,
	comments: 38,
	chats: 0,
	skipped: 0,
	estimated_tokens: 40000,
	left_today: 260000,
	run_cap: 120000,
	...extra,
});

const activeRun = (extra: Record<string, unknown> = {}) => ({
	id: 5,
	status: 'running',
	mode: 'sources',
	stage: 'reading',
	progress_done: 3,
	progress_total: 8,
	tokens_used: 0,
	sources_skipped: 0,
	error: null,
	created_at: '2026-11-14T10:00:00Z',
	finished_at: null,
	...extra,
});

const Modal = loadSfc('SummarizeWorkspaceModal.vue');

const open = async () => {
	const mounted = mountComponent(Modal, { workspaceId: 3, topics });
	await flush();
	return mounted;
};

const runButton = (host: HTMLElement) =>
	byTestId(host, 'summarize-run') as HTMLButtonElement;

beforeEach(() => {
	jest.useRealTimers();
	mockPreview.mockReset();
	mockStart.mockReset();
	mockCurrent.mockReset();
	mockCancel.mockReset();
	mockCategories.mockReset();
	mockSubscribe.mockReset();
	mockUnsubscribe.mockReset();
	mockPreview.mockResolvedValue(preview());
	mockCurrent.mockResolvedValue(null);
	mockCategories.mockResolvedValue([{ id: 11, title: 'Backend' }]);
	mockSubscribe.mockReturnValue('sub-1');
	document.body.innerHTML = '';
});

test('runDisabled_overLimitOrNoSources', async () => {
	const { host, unmount } = await open();
	expect(byTestId(host, 'summarize-preview')!.textContent).toBe(
		'124 tasks, 38 comments, about 40k tokens, limit left today: 260k',
	);
	expect(runButton(host).disabled).toBe(false);
	unmount();

	mockPreview.mockResolvedValue(
		preview({ estimated_tokens: 300000, left_today: 260000 }),
	);
	const over = await open();
	expect(runButton(over.host).disabled).toBe(true);
	over.unmount();

	mockPreview.mockResolvedValue(preview({ tasks: 0, comments: 0 }));
	const none = await open();
	expect(runButton(none.host).disabled).toBe(true);
	none.unmount();
});

test('myChatsOffByDefault', async () => {
	const { host, unmount } = await open();
	const chats = byTestId(host, 'summarize-my-chats') as HTMLInputElement;
	const tasks = byTestId(host, 'summarize-tasks') as HTMLInputElement;
	const comments = byTestId(host, 'summarize-comments') as HTMLInputElement;
	expect(chats.checked).toBe(false);
	expect(tasks.checked && tasks.disabled).toBe(true);
	expect(comments.checked && comments.disabled).toBe(true);
	expect(mockPreview).toHaveBeenLastCalledWith(
		3,
		expect.objectContaining({ mode: 'sources', days: 30, my_chats: false }),
	);

	chats.click();
	await flush();
	expect(mockPreview).toHaveBeenLastCalledWith(
		3,
		expect.objectContaining({ my_chats: true }),
	);
	unmount();
});

test('alreadyRunningMessage', async () => {
	mockCurrent.mockResolvedValue(activeRun());
	const current = await open();
	expect(current.host.textContent).toContain('A summary is already running');
	expect(current.host.textContent).toContain('Reading sources 3/8');
	expect(byTestId(current.host, 'summarize-run')).toBeNull();
	current.unmount();

	mockCurrent.mockResolvedValue(null);
	mockStart.mockRejectedValue({
		response: { status: 409, data: { error: 'already_running' } },
	});
	const conflict = await open();
	runButton(conflict.host).click();
	await flush();
	expect(conflict.host.textContent).toContain('A summary is already running');
	conflict.unmount();
});

test('progress comes from events and polling stops on unmount', async () => {
	jest.useFakeTimers();
	mockStart.mockResolvedValue(activeRun({ progress_done: 0 }));
	const { host, unmount } = mountComponent(Modal, { workspaceId: 3, topics });
	await flush();
	runButton(host).click();
	await flush();
	mockCurrent.mockResolvedValue(activeRun({ progress_done: 5 }));
	const handlers = mockSubscribe.mock.calls[0][1];
	handlers.onMemorySummarizeProgress({
		run_id: 5,
		stage: 'reading',
		done: 4,
		total: 8,
	});
	await flush();
	expect(host.textContent).toContain('Reading sources 4/8');
	handlers.onMemorySummarizeProgress({
		run_id: 5,
		stage: 'extracting',
		done: 0,
		total: 0,
	});
	await flush();
	expect(host.textContent).toContain('Extracting facts');

	mockCurrent.mockClear();
	jest.advanceTimersByTime(5000);
	await flush();
	expect(mockCurrent).toHaveBeenCalledTimes(1);

	unmount();
	expect(mockUnsubscribe).toHaveBeenCalledWith('App.User.7', 'sub-1');
	mockCurrent.mockClear();
	jest.advanceTimersByTime(20000);
	expect(mockCurrent).not.toHaveBeenCalled();
});

test('extend needs a topic and sends it', async () => {
	const { host, unmount } = await open();
	buttonWithText(host, 'Extend a topic')!.click();
	await flush();
	expect(runButton(host).disabled).toBe(true);
	buttonWithText(host, 'Decisions')!.click();
	await flush();
	expect(mockPreview).toHaveBeenLastCalledWith(
		3,
		expect.objectContaining({ mode: 'extend', topic: 'decisions' }),
	);
	expect(runButton(host).disabled).toBe(false);
	unmount();
});
