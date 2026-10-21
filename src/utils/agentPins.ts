import type { AgentPin, AgentPinLimits } from '@/types/agent';

export const DEFAULT_PIN_LIMITS: AgentPinLimits = {
	max_pins: 20,
	max_chars: 500,
	max_total_chars: 4000,
};

const SAVE_FAILED = 'Could not save the pinned fact. Please try again.';

export const pinErrorMessage = (
	status: number | undefined,
	serverMessage?: string,
): string => {
	if (status === 422) {
		return serverMessage?.trim() || 'The pinned fact is not valid.';
	}
	return SAVE_FAILED;
};

export const pinTextFromMessage = (
	content: string,
	maxChars: number = DEFAULT_PIN_LIMITS.max_chars,
): string => content.replace(/\s+/g, ' ').trim().slice(0, maxChars).trim();

const pinsTotalChars = (pins: AgentPin[]): number =>
	pins.reduce((sum, pin) => sum + pin.content.length, 0);

export const pinCharsLeft = (
	pins: AgentPin[],
	limits: AgentPinLimits,
	editingId: number | null,
): number =>
	Math.max(
		0,
		Math.min(
			limits.max_chars,
			limits.max_total_chars -
				pinsTotalChars(pins.filter((pin) => pin.id !== editingId)),
		),
	);

const fitsRoom = (
	pins: AgentPin[],
	limits: AgentPinLimits,
	editingId: number | null,
	text: string,
): boolean => {
	const length = text.trim().length;
	return length > 0 && length <= pinCharsLeft(pins, limits, editingId);
};

export const canAddPin = (
	pins: AgentPin[],
	limits: AgentPinLimits,
	text: string,
): boolean =>
	pins.length < limits.max_pins && fitsRoom(pins, limits, null, text);

export const canEditPin = (
	pins: AgentPin[],
	limits: AgentPinLimits,
	pinId: number,
	text: string,
): boolean => fitsRoom(pins, limits, pinId, text);

export const pinLimitHint = (
	pins: AgentPin[],
	limits: AgentPinLimits,
	text: string,
	editingId: number | null,
): string => {
	if (editingId === null && pins.length >= limits.max_pins) {
		return `You can pin up to ${limits.max_pins} facts. Remove one to add another.`;
	}
	const left = pinCharsLeft(pins, limits, editingId);
	return text.trim().length > left
		? `Only ${left} characters left for pinned facts.`
		: '';
};
