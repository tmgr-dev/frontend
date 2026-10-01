import $axios from '@/plugins/axios';

export interface VerifyEmailResult {
	status: 'verified' | 'already_verified';
}

export interface ResendVerificationResult {
	status: 'sent' | 'already_verified';
}

export interface ConfirmSocialLinkResult {
	status: 'linked';
	provider: string;
}

export const verifyEmail = async (
	token: string,
): Promise<VerifyEmailResult> => {
	const {
		data: { data },
	} = await $axios.post('auth/email/verify', { token });
	return data;
};

export const resendVerificationEmail =
	async (): Promise<ResendVerificationResult> => {
		const {
			data: { data },
		} = await $axios.post('auth/email/resend');
		return data;
	};

export const confirmSocialLink = async (
	token: string,
): Promise<ConfirmSocialLinkResult> => {
	const {
		data: { data },
	} = await $axios.post('auth/social-link/confirm', { token });
	return data;
};
