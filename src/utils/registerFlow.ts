import { t } from '@/i18n';

export type RegisterForm = {
	password: string;
	password_confirmation: string;
};

export type RegisterFailure = {
	message: string;
	emailTaken: boolean;
	errors: Record<string, string[]>;
};

type ErrorShape = {
	response?: { status?: number; data?: any };
};

export const describeRegisterError = (error: unknown): RegisterFailure => {
	const response = (error as ErrorShape | null)?.response;
	const none = { message: '', emailTaken: false, errors: {} };
	if (!response)
		return {
			...none,
			message: t('auth.register.errors.offline'),
		};
	const { status, data } = response;
	if (status === 409 || data?.error === 'email_taken')
		return {
			...none,
			emailTaken: true,
			message: t('auth.register.errors.emailTaken'),
		};
	if (status === 429)
		return {
			...none,
			message: t('auth.register.errors.tooManyAttempts'),
		};
	if (status === 422 && data?.errors && typeof data.errors === 'object')
		return { ...none, errors: data.errors };
	return { ...none, message: t('auth.register.errors.generic') };
};

export const submitRegistration = async <F extends RegisterForm>(
	form: F,
	register: (form: F) => Promise<unknown>,
): Promise<{ ok: true } | ({ ok: false } & RegisterFailure)> => {
	if (form.password !== form.password_confirmation)
		return {
			ok: false,
			message: '',
			emailTaken: false,
			errors: { password_confirmation: [t('auth.register.errors.mismatch')] },
		};
	try {
		await register(form);
		return { ok: true };
	} catch (error) {
		return { ok: false, ...describeRegisterError(error) };
	}
};
