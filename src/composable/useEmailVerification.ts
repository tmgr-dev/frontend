import { httpErrorInfo } from '@/utils/emailVerification';
import { ref } from 'vue';

export type ResendState =
	| 'idle'
	| 'sending'
	| 'sent'
	| 'already_verified'
	| 'rate_limited'
	| 'error';

export type VerifyState =
	| 'verifying'
	| 'success'
	| 'already_verified'
	| 'expired'
	| 'invalid'
	| 'wrong_account'
	| 'error';

export type LinkConfirmState =
	| 'ready'
	| 'confirming'
	| 'linked'
	| 'expired'
	| 'invalid'
	| 'wrong_account'
	| 'error';

export const createResendController = (deps: {
	resend: () => Promise<{ status: string }>;
	onAlreadyVerified?: () => unknown;
}) => {
	const state = ref<ResendState>('idle');
	const retryAfter = ref(0);

	const send = async () => {
		if (state.value === 'sending') return;
		state.value = 'sending';
		try {
			const result = await deps.resend();
			if (result.status === 'already_verified') {
				state.value = 'already_verified';
				await deps.onAlreadyVerified?.();
			} else {
				state.value = 'sent';
			}
		} catch (error) {
			const info = httpErrorInfo(error);
			if (info.status === 429) {
				retryAfter.value = Number(info.retryAfter) || 0;
				state.value = 'rate_limited';
			} else {
				state.value = 'error';
			}
		}
	};

	return { state, retryAfter, send };
};

export const createVerifyEmail = (deps: {
	verify: (token: string) => Promise<{ status: string }>;
	resend: () => Promise<{ status: string }>;
	refreshUser: () => Promise<unknown>;
}) => {
	const state = ref<VerifyState>('verifying');
	const refreshUser = () => deps.refreshUser().catch(() => undefined);
	const resend = createResendController({
		resend: deps.resend,
		onAlreadyVerified: refreshUser,
	});

	const run = async (token: string) => {
		if (!token) {
			state.value = 'invalid';
			return;
		}
		state.value = 'verifying';
		try {
			const result = await deps.verify(token);
			state.value =
				result.status === 'already_verified' ? 'already_verified' : 'success';
			await refreshUser();
		} catch (error) {
			const { status, code } = httpErrorInfo(error);
			if (status === 403 && code === 'wrong_account') {
				state.value = 'wrong_account';
			} else if (status === 422 && code === 'expired_token') {
				state.value = 'expired';
			} else if (status === 422 && code === 'invalid_token') {
				state.value = 'invalid';
			} else {
				state.value = 'error';
			}
		}
	};

	return { state, run, resend };
};

export const createLinkConfirm = (deps: {
	confirm: (token: string) => Promise<{ status: string; provider?: string }>;
}) => {
	const state = ref<LinkConfirmState>('ready');
	const provider = ref('');

	const run = async (token: string) => {
		if (!token) {
			state.value = 'invalid';
			return;
		}
		state.value = 'confirming';
		try {
			const result = await deps.confirm(token);
			provider.value = result.provider ?? '';
			state.value = 'linked';
		} catch (error) {
			const { status, code } = httpErrorInfo(error);
			if (status === 403 && code === 'wrong_account') {
				state.value = 'wrong_account';
			} else if (status === 422 && code === 'expired_token') {
				state.value = 'expired';
			} else if (status === 422 && code === 'invalid_token') {
				state.value = 'invalid';
			} else {
				state.value = 'error';
			}
		}
	};

	return { state, provider, run };
};
