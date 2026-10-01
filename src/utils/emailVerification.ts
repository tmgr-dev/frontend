interface HttpErrorLike {
	response?: {
		status?: number;
		data?: { error?: string; message?: string; retry_after?: number };
	};
}

const GOOGLE_LINK_NOTICE =
	'We emailed you a confirmation link. Open it while signed in to your tmgr.dev account (password or another sign-in method), confirm, then sign in with Google again.';

const DISMISS_KEY = 'email.verify.banner.dismissed';
const RETURN_KEY = 'auth.return.to';

export const httpErrorInfo = (error: unknown) => {
	const response = (error as HttpErrorLike | null)?.response;
	return {
		status: response?.status,
		code: response?.data?.error,
		message: response?.data?.message,
		retryAfter: response?.data?.retry_after,
	};
};

export const shouldShowVerifyBanner = (
	user: { email_verified?: boolean } | null | undefined,
	dismissed: boolean,
): boolean => user?.email_verified === false && !dismissed;

export const googleLinkConfirmationMessage = (
	error: unknown,
): string | null => {
	const { status, code, message } = httpErrorInfo(error);
	if (status !== 409 || code !== 'email_link_confirmation_required') {
		return null;
	}
	return GOOGLE_LINK_NOTICE;
};

export const errorMessageFrom = (error: unknown, fallback: string): string => {
	const { status, message } = httpErrorInfo(error);
	return status === 403 && message ? message : fallback;
};

export const retryAfterLabel = (seconds: number): string => {
	if (!(seconds > 0)) return 'a moment';
	if (seconds >= 60) {
		const minutes = Math.ceil(seconds / 60);
		return `${minutes} minute${minutes === 1 ? '' : 's'}`;
	}
	return `${seconds} second${seconds === 1 ? '' : 's'}`;
};

export const isSafeReturnPath = (path: string | null | undefined): boolean =>
	typeof path === 'string' &&
	path.startsWith('/') &&
	!path.startsWith('//') &&
	!path.startsWith('/\\');

export const isBannerDismissed = (): boolean => {
	try {
		return sessionStorage.getItem(DISMISS_KEY) !== null;
	} catch {
		return false;
	}
};

export const dismissBanner = (): void => {
	try {
		sessionStorage.setItem(DISMISS_KEY, '1');
	} catch {}
};

export const saveReturnPath = (path: string): void => {
	if (!isSafeReturnPath(path)) return;
	try {
		sessionStorage.setItem(RETURN_KEY, path);
	} catch {}
};

export const takeReturnPath = (): string | null => {
	try {
		const path = sessionStorage.getItem(RETURN_KEY);
		sessionStorage.removeItem(RETURN_KEY);
		return isSafeReturnPath(path) ? path : null;
	} catch {
		return null;
	}
};

export const withoutTokenQuery = (
	query: Record<string, unknown>,
): Record<string, unknown> => {
	const { token: _token, ...rest } = query;
	return rest;
};
