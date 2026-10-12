import $axios from '@/plugins/axios';
import type { Ref } from 'vue';
import type { OnboardingState } from './types';

export const setHubHidden = async (
	onboarding: Readonly<Ref<OnboardingState | null>>,
	hidden: boolean,
): Promise<void> => {
	await $axios.put('onboarding/preferences', { hub_hidden: hidden });
	const state = onboarding as Ref<OnboardingState | null>;
	if (state.value) state.value = { ...state.value, hub_hidden: hidden };
};
