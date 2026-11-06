type EnterKey = Pick<KeyboardEvent, 'shiftKey' | 'isComposing' | 'keyCode'>;

/**
 * Enter sends the comment, Shift+Enter adds a line break (TM-686). Enter that commits an IME
 * composition must not send: WebKit reports it with `isComposing` already false but keyCode 229.
 */
export const isSendEnter = (e: EnterKey): boolean =>
	!e.shiftKey && !e.isComposing && e.keyCode !== 229;

export const isEnterSend = (
	e: EnterKey & Pick<KeyboardEvent, 'key'>,
): boolean => e.key === 'Enter' && isSendEnter(e);

export const autosizeTextarea = (
	el: HTMLTextAreaElement | null | undefined,
	maxHeight = 160,
): void => {
	if (!el || !el.scrollHeight) return;
	el.style.height = 'auto';
	el.style.height = `${Math.min(el.scrollHeight, maxHeight)}px`;
	el.style.overflowY = el.scrollHeight > maxHeight ? 'auto' : 'hidden';
};
