import {
	errorText,
	usageConsent,
	usageConsentSet,
	usageForget,
	type UsageConsent,
	type UsageConsentChange,
} from '@/usage/api';
import { isDesktopApp } from '@/utils/desktop';
import { computed, ref } from 'vue';

export type ConsentGate = 'unsupported' | 'ask' | 'off' | 'granted';

export const consentGate = (c: UsageConsent | null): ConsentGate | null => {
	if (!c) return null;
	if (!c.supported) return 'unsupported';
	if (c.state === 'unset') return 'ask';
	return c.state === 'granted' ? 'granted' : 'off';
};

export const readingNote = (state: UsageConsent['state']): string =>
	({
		granted:
			'On: TMGR reads these logs on this device to show cost and limits.',
		denied: "Off: TMGR doesn't read these logs for AI usage on this device.",
		unset:
			"Not chosen yet: the desktop app and background jobs don't read them for AI usage, tmgr usage commands you run yourself still do.",
	})[state];

export const FORGET_DONE = 'Deleted. Reading AI tool logs is off.';

export const useUsageConsent = (initial: UsageConsent | null = null) => {
	const consent = ref<UsageConsent | null>(initial);
	const busy = ref(false);
	const error = ref<string | null>(null);
	const message = ref('');
	const gate = computed(() => consentGate(consent.value));

	const run = async (call: () => Promise<UsageConsent>): Promise<boolean> => {
		busy.value = true;
		error.value = null;
		message.value = '';
		try {
			consent.value = await call();
			return true;
		} catch (e) {
			error.value = errorText(e);
			return false;
		} finally {
			busy.value = false;
		}
	};

	const load = (): Promise<boolean> =>
		isDesktopApp() ? run(usageConsent) : Promise.resolve(false);

	const save = (change: UsageConsentChange): Promise<boolean> =>
		busy.value ? Promise.resolve(false) : run(() => usageConsentSet(change));

	const forget = async (): Promise<boolean> => {
		if (busy.value) return false;
		const ok = await run(usageForget);
		if (ok) {
			message.value = FORGET_DONE;
			return true;
		}
		const failure = error.value;
		await load();
		error.value = failure;
		return false;
	};

	return { consent, busy, error, message, gate, load, save, forget };
};
