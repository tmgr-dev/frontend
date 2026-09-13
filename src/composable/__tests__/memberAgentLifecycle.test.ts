import * as agentApi from '@/actions/tmgr/agent';
import * as memberApi from '@/actions/tmgr/dashboard';
import { nextTick, reactive, ref } from 'vue';
import { useAgentChat } from '../useAgentChat';
import { useMemberPage } from '../useMemberPage';
const route = reactive({ query: {} as Record<string, string> });
jest.mock('vue-router', () => ({
	useRoute: () => route,
	useRouter: () => ({ replace: jest.fn() }),
}));
jest.mock('@/actions/tmgr/dashboard', () => ({
	getMemberStats: jest.fn(),
	getMemberTasks: jest.fn(),
}));
jest.mock('@/actions/tmgr/agent', () => ({
	getOrCreateConversation: jest.fn(),
	getAgentMessages: jest.fn(),
	sendAgentMessage: jest.fn(),
	startNewConversation: jest.fn(),
}));
jest.mock('../usePusher', () => ({ usePusher: () => ({}) }));
const deferred = () => {
	let resolve!: (v: any) => void;
	let reject!: (v: any) => void;
	const promise = new Promise<any>((a, b) => {
		resolve = a;
		reject = b;
	});
	return { promise, resolve, reject };
};
beforeEach(() => jest.clearAllMocks());
test('member switching supersedes pending stats and tasks; errors settle loading', async () => {
	const stats = deferred(),
		tasks = deferred(),
		ws = ref(1);
	(memberApi.getMemberStats as jest.Mock)
		.mockReturnValueOnce(stats.promise)
		.mockResolvedValue({ success: true, data: { user_id: 2 } });
	(memberApi.getMemberTasks as jest.Mock)
		.mockReturnValueOnce(tasks.promise)
		.mockResolvedValue({
			success: true,
			data: { data: [{ id: 2 }], total: 1 },
		});
	const m = useMemberPage(
		() => ws.value,
		() => 1,
	);
	const old = m.refresh();
	ws.value = 2;
	await nextTick();
	await Promise.resolve();
	stats.resolve({ success: true, data: { user_id: 1 } });
	tasks.resolve({ success: true, data: { data: [{ id: 1 }], total: 1 } });
	await old;
	expect(m.stats.value).toEqual({ user_id: 2 });
	expect(m.tasks.value).toEqual([{ id: 2 }]);
	(memberApi.getMemberStats as jest.Mock).mockRejectedValue(
		new Error('offline'),
	);
	(memberApi.getMemberTasks as jest.Mock).mockRejectedValue(
		new Error('offline'),
	);
	await m.refresh();
	expect(m.statsLoading.value).toBe(false);
	expect(m.tasksLoading.value).toBe(false);
	expect(m.error.value).toBe('offline');
});
test('chat ignores a late conversation before fetching its messages', async () => {
	const old = deferred();
	(agentApi.getOrCreateConversation as jest.Mock)
		.mockReturnValueOnce(old.promise)
		.mockResolvedValue({ id: 2, workspace_id: 2 });
	(agentApi.getAgentMessages as jest.Mock).mockResolvedValue([]);
	const chat = useAgentChat();
	const first = chat.load(1);
	await chat.load(2);
	old.resolve({ id: 1, workspace_id: 1 });
	await first;
	expect(chat.state.value.conversationId).toBe(2);
	expect(agentApi.getAgentMessages).toHaveBeenCalledTimes(1);
	expect(chat.loading.value).toBe(false);
});
test('late send rejection cannot replace new workspace error or loading', async () => {
	(agentApi.getOrCreateConversation as jest.Mock)
		.mockResolvedValueOnce({ id: 1, workspace_id: 1 })
		.mockResolvedValueOnce({ id: 2, workspace_id: 2 });
	(agentApi.getAgentMessages as jest.Mock).mockResolvedValue([]);
	const sent = deferred();
	(agentApi.sendAgentMessage as jest.Mock).mockReturnValue(sent.promise);
	const chat = useAgentChat();
	await chat.load(1);
	const sending = chat.send('old workspace');
	await chat.load(2);
	sent.reject(new Error('old error'));
	await sending;
	expect(chat.error.value).toBeNull();
	expect(chat.sending.value).toBe(false);
	expect(chat.state.value.conversationId).toBe(2);
});
