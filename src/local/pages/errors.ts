import { LocalHttpError } from '../types';

export const pageError = (status: number, error: string, message: string, extra: Record<string, unknown> = {}) =>
	new LocalHttpError(status, message, undefined, { message, error, ...extra });

export const forbidden = (error: string, message: string) => pageError(403, error, message);
export const notFound = (error: string, message: string) => pageError(404, error, message);
export const unprocessable = (error: string, message: string, extra: Record<string, unknown> = {}) =>
	pageError(422, error, message, extra);
export const tooLarge = () => pageError(413, 'page_too_large', 'Page body exceeds 1 MB');
export const conflict = (current: unknown) =>
	pageError(409, 'page_conflict', 'Page was changed', { data: current });
export const pageNotFound = () => notFound('page_not_found', 'Page not found');
