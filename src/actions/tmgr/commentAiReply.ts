import store from '@/store';
import { ref } from 'vue';
import { updateUserSettingsV2 } from './user';

export const COMMENT_AI_REPLY_KEY = 'comment_ai_reply';

const sessionValue = ref(false);

const isOn = (value: unknown) =>
	value === true || value === 1 || value === '1' || value === 'true';

const withValue = (settings: any[], value: unknown) =>
	settings.map((s: any) =>
		s?.key === COMMENT_AI_REPLY_KEY ? { ...s, value } : s,
	);

export const isCommentAiReplyOn = (): boolean => {
	const setting = store.getters.userSettingByKey(COMMENT_AI_REPLY_KEY);
	return setting ? isOn(setting.value) : sessionValue.value;
};

export const setCommentAiReply = async (on: boolean): Promise<void> => {
	const user = (store.state as any).user;
	const setting = store.getters.userSettingByKey(COMMENT_AI_REPLY_KEY);
	if (!setting || !Array.isArray(user?.settings)) {
		sessionValue.value = on;
		return;
	}
	const value = on ? '1' : '0';
	const payload = user.settings.map((s: any) => ({
		id: s.id,
		value: s.key === COMMENT_AI_REPLY_KEY ? value : s.value,
	}));
	store.commit('setUser', { ...user, settings: withValue(user.settings, value) });
	try {
		store.commit('setUser', await updateUserSettingsV2(payload));
	} catch (error) {
		const current = (store.state as any).user;
		store.commit('setUser', {
			...current,
			settings: withValue(current?.settings ?? [], setting.value),
		});
		throw error;
	}
};
