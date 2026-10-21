type EnterKey = Pick<KeyboardEvent, 'shiftKey' | 'isComposing' | 'keyCode'>;

/**
 * Enter sends the comment, Shift+Enter adds a line break (TM-686). Enter that commits an IME
 * composition must not send: WebKit reports it with `isComposing` already false but keyCode 229.
 */
export const isSendEnter = (e: EnterKey): boolean =>
	!e.shiftKey && !e.isComposing && e.keyCode !== 229;
