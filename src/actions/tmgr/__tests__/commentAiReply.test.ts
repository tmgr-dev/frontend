jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { put: jest.fn() },
}));
jest.mock('@/store', () => {
	const state: any = { user: null };
	const byKey = (key: string) =>
		state.user?.settings?.find((s: any) => s.key === key);
	return {
		__esModule: true,
		default: {
			state,
			getters: {
				get userSettingByKey() {
					return byKey;
				},
				defaultWorkspaceId: 1,
			},
			commit: jest.fn((name: string, user: any) => {
				if (name === 'setUser') state.user = user;
			}),
		},
	};
});

import $axios from '@/plugins/axios';
import store from '@/store';
import { isCommentAiReplyOn, setCommentAiReply } from '../commentAiReply';

const userWith = (settings: any[]) => ({ id: 1, settings });

beforeEach(() => {
	jest.clearAllMocks();
	(store.state as any).user = null;
});

test('without the setting in the catalog the toggle lives in memory and sends nothing', async () => {
	(store.state as any).user = userWith([
		{ id: 6, key: 'dark_mode', value: '1' },
	]);
	expect(isCommentAiReplyOn()).toBe(false);
	await setCommentAiReply(true);
	expect(isCommentAiReplyOn()).toBe(true);
	expect($axios.put).not.toHaveBeenCalled();
	await setCommentAiReply(false);
	expect(isCommentAiReplyOn()).toBe(false);
});

test('reads the stored value of the user setting', () => {
	(store.state as any).user = userWith([
		{ id: 20, key: 'comment_ai_reply', value: '1' },
	]);
	expect(isCommentAiReplyOn()).toBe(true);
	(store.state as any).user = userWith([
		{ id: 20, key: 'comment_ai_reply', value: '0' },
	]);
	expect(isCommentAiReplyOn()).toBe(false);
});

test('saves the full settings list with only the AI reply value changed', async () => {
	(store.state as any).user = userWith([
		{ id: 6, key: 'dark_mode', value: '1' },
		{ id: 20, key: 'comment_ai_reply', value: '0' },
	]);
	const saved = userWith([
		{ id: 6, key: 'dark_mode', value: '1' },
		{ id: 20, key: 'comment_ai_reply', value: '1' },
	]);
	($axios.put as jest.Mock).mockResolvedValue({ data: { data: saved } });
	const pending = setCommentAiReply(true);
	expect(isCommentAiReplyOn()).toBe(true);
	await pending;
	expect($axios.put).toHaveBeenCalledWith('v2/user/settings', [
		{ id: 6, value: '1' },
		{ id: 20, value: '1' },
	]);
	expect((store.state as any).user).toEqual(saved);
});

test('reverts the toggle when saving fails', async () => {
	(store.state as any).user = userWith([
		{ id: 20, key: 'comment_ai_reply', value: '0' },
	]);
	($axios.put as jest.Mock).mockRejectedValue(new Error('offline'));
	await expect(setCommentAiReply(true)).rejects.toThrow('offline');
	expect(isCommentAiReplyOn()).toBe(false);
});
