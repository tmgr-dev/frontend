import { isDesktopApp } from '@/utils/desktop';

const MOBILE_UA = /Android|iPhone|iPad|iPod/i;

export const canOpenInApp = (): boolean =>
	!isDesktopApp() && !MOBILE_UA.test(navigator.userAgent);

/** Starts the write synchronously: WKWebView drops the user gesture across an await. */
export const writeClipboard = async (
	value: string | Promise<string>,
): Promise<void> => {
	const clipboard = navigator.clipboard;
	if (typeof value !== 'string') value.catch(() => undefined);
	if (!clipboard) throw new Error('Clipboard not supported');
	if (typeof value === 'string') return clipboard.writeText(value);
	if (typeof ClipboardItem !== 'undefined' && clipboard.write) {
		const blob = value.then((text) => new Blob([text], { type: 'text/plain' }));
		blob.catch(() => undefined);
		return clipboard.write([new ClipboardItem({ 'text/plain': blob })]);
	}
	return clipboard.writeText(await value);
};
