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

export const MISMATCH_MESSAGE = 'The password confirmation does not match.';

export const describeRegisterError = (error: unknown): RegisterFailure => {
	const response = (error as ErrorShape | null)?.response;
	const none = { message: '', emailTaken: false, errors: {} };
	if (!response)
		return {
			...none,
			message:
				"We couldn't reach the server. Check your connection and try again. If you already created this account, sign in instead.",
		};
	const { status, data } = response;
	if (status === 409 || data?.error === 'email_taken')
		return {
			...none,
			emailTaken: true,
			message: 'An account with this email already exists.',
		};
	if (status === 429)
		return {
			...none,
			message: 'Too many attempts. Wait a minute and try again.',
		};
	if (status === 422 && data?.errors && typeof data.errors === 'object')
		return { ...none, errors: data.errors };
	return { ...none, message: 'Could not create the account. Try again.' };
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
			errors: { password_confirmation: [MISMATCH_MESSAGE] },
		};
	try {
		await register(form);
		return { ok: true };
	} catch (error) {
		return { ok: false, ...describeRegisterError(error) };
	}
};
