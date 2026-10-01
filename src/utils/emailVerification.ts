interface HttpErrorLike {
	response?: {
		status?: number;
		data?: { error?: string; message?: string; retry_after?: number };
	};
}

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
	return (
		message ||
		'We emailed you a link to confirm linking your Google account. Open it, then sign in with Google again.'
	);
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
