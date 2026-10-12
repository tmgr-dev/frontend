import { onScopeDispose, readonly, ref } from 'vue';
import { consentStore, type ConsentRecord } from './consent';

export const useConsent = () => {
	const store = consentStore();
	const record = ref<ConsentRecord>({ ...store.get() });
	const saveFailed = ref(store.saveFailed());
	const stop = store.subscribe((next) => {
		record.value = { ...next };
		saveFailed.value = store.saveFailed();
	});
	onScopeDispose(stop);
	return {
		consent: readonly(record),
		saveFailed: readonly(saveFailed),
		setAnalytics: (analytics: 'granted' | 'denied' | null) =>
			store.set({ analytics }),
		setUsage: (usage: 'granted' | 'denied' | null) => store.set({ usage }),
		reopenChoice: () => store.set({ analytics: null }),
	};
};
