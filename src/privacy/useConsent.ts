import { usageConsent } from '@/telemetry/consent';
import { onScopeDispose, readonly, ref } from 'vue';
import { consentStore, type ConsentRecord, type ConsentValue } from './consent';

export interface ConsentView extends ConsentRecord {
	usage: ConsentValue;
}

export const useConsent = () => {
	const analytics = consentStore();
	const usage = usageConsent();
	const view = (): ConsentView => {
		const state = usage.read();
		return {
			...analytics.get(),
			usage: state.answered ? (state.on ? 'granted' : 'denied') : null,
		};
	};
	const record = ref<ConsentView>(view());
	const saveFailed = ref(usage.saveFailed());
	const refresh = () => {
		record.value = view();
		saveFailed.value = usage.saveFailed();
	};
	const stopAnalytics = analytics.subscribe(refresh);
	const stopUsage = usage.subscribe(refresh);
	onScopeDispose(() => {
		stopAnalytics();
		stopUsage();
	});
	return {
		consent: readonly(record),
		saveFailed: readonly(saveFailed),
		setAnalytics: (value: ConsentValue) => analytics.set({ analytics: value }),
		setUsage: (value: 'granted' | 'denied') => usage.set(value === 'granted'),
		reopenChoice: () => analytics.set({ analytics: null }),
	};
};
