import {
	DesktopAuthProvider,
	beginDesktopAuth,
	clearPendingDesktopAuth,
} from '@/utils/desktopAuth';
import { ref } from 'vue';

export type DesktopAuthStatus = 'idle' | 'waiting' | 'completing' | 'error';

export const desktopAuthStatus = ref<DesktopAuthStatus>('idle');
export const desktopAuthError = ref('');

export const failDesktopSocialLogin = (message: string): void => {
	desktopAuthStatus.value = 'error';
	desktopAuthError.value = message;
};

export const startDesktopSocialLogin = async (
	provider: DesktopAuthProvider,
): Promise<void> => {
	try {
		const url = await beginDesktopAuth(
			provider,
			import.meta.env.VITE_API_BASE_URL,
		);
		desktopAuthError.value = '';
		desktopAuthStatus.value = 'waiting';
		// The desktop shell opens external navigations in the system browser.
		window.location.href = url;
	} catch (error) {
		failDesktopSocialLogin(
			error instanceof Error ? error.message : 'Could not start sign-in.',
		);
	}
};

export const cancelDesktopSocialLogin = (): void => {
	clearPendingDesktopAuth();
	desktopAuthStatus.value = 'idle';
	desktopAuthError.value = '';
};
