import {
	MAX_INPUT_VALUE,
	MAX_TEXTAREA_VALUE,
	calendarDate,
	clampDebounce,
	cutCodePoints,
} from '@/pluginSystem/uiTree';
import { segmentedArgs } from './pluginViewClasses';

export const inputArgs = (args: unknown, typed: string) =>
	segmentedArgs(args, typed.slice(0, MAX_INPUT_VALUE));

export const selectArgs = (
	node: { options: { value: string }[]; args?: unknown },
	chosen: unknown,
) =>
	typeof chosen === 'string' &&
	node.options.some((option) => option.value === chosen)
		? segmentedArgs(node.args, chosen)
		: null;

export const checkboxArgs = (args: unknown, checked: unknown) =>
	segmentedArgs(args, checked === true);

export const cutTextareaValue = (typed: string) =>
	cutCodePoints(typed, MAX_TEXTAREA_VALUE);

export const unsentAfterSend = (succeeded: boolean, unsentNow: boolean) =>
	unsentNow || !succeeded;

export const textareaArgs = (args: unknown, typed: string) =>
	segmentedArgs(args, cutCodePoints(typed, MAX_TEXTAREA_VALUE));

export const dateArgs = (
	node: { min?: string; max?: string; args?: unknown },
	raw: unknown,
) => {
	if (raw === '') return segmentedArgs(node.args, null);
	const picked = calendarDate(raw);
	return picked !== null &&
		(node.min === undefined || picked >= node.min) &&
		(node.max === undefined || picked <= node.max)
		? segmentedArgs(node.args, picked)
		: null;
};

export const debounceDelay = clampDebounce;

type TimerId = ReturnType<typeof setTimeout>;

export const createDebouncer = (
	setTimer: (fn: () => void, ms: number) => TimerId = setTimeout,
	clearTimer: (id: TimerId) => void = clearTimeout,
) => {
	let id: TimerId | undefined;
	const cancel = () => {
		if (id !== undefined) clearTimer(id);
		id = undefined;
	};
	return {
		schedule(fn: () => void, ms: number) {
			cancel();
			id = setTimer(() => {
				id = undefined;
				fn();
			}, debounceDelay(ms));
		},
		cancel,
		pending: () => id !== undefined,
	};
};

type ControlIdentity = {
	type: string;
	name?: string;
	command?: string;
	args?: unknown;
};

export const sameControl = (
	a: ControlIdentity | undefined,
	b: ControlIdentity | undefined,
) =>
	a !== undefined &&
	b !== undefined &&
	a.type === b.type &&
	a.name === b.name &&
	a.command === b.command &&
	JSON.stringify(a.args ?? null) === JSON.stringify(b.args ?? null);

export const nextInputText = (
	node: ControlIdentity & { value?: unknown },
	previous: ControlIdentity | undefined,
	focused: boolean,
	current: string,
): string =>
	sameControl(node, previous) && focused
		? current
		: node.type === 'input' && typeof node.value === 'string'
			? node.value
			: '';

export const nextTextareaText = (
	node: ControlIdentity & { value?: unknown },
	previous: ControlIdentity | undefined,
	unsent: boolean,
	current: string,
): string =>
	sameControl(node, previous) && unsent
		? current
		: node.type === 'textarea' && typeof node.value === 'string'
			? node.value
			: '';

export const isSendShortcut = (event: {
	key: string;
	metaKey: boolean;
	ctrlKey: boolean;
	altKey: boolean;
	shiftKey: boolean;
	isComposing: boolean;
}) =>
	event.key === 'Enter' &&
	!event.isComposing &&
	!event.altKey &&
	!event.shiftKey &&
	(event.metaKey || event.ctrlKey);

export const sendKeyAction = (
	event: Parameters<typeof isSendShortcut>[0] & { repeat: boolean },
	running: boolean,
): 'ignore' | 'swallow' | 'send' =>
	!isSendShortcut(event)
		? 'ignore'
		: event.repeat || running
			? 'swallow'
			: 'send';

export const pluginControlAttrs = {
	autocomplete: 'off',
	'data-lpignore': 'true',
	'data-1p-ignore': 'true',
	'data-bwignore': 'true',
	'data-form-type': 'other',
} as const;

export const pluginInputTextAttrs = {
	...pluginControlAttrs,
	type: 'text',
	autocorrect: 'off',
	autocapitalize: 'off',
	spellcheck: 'false',
} as const;

export const pluginTextareaAttrs = {
	...pluginControlAttrs,
	autocorrect: 'off',
	autocapitalize: 'off',
	spellcheck: 'false',
} as const;

export const pluginControlLabel = (pluginName: string): string =>
	`Plugin: ${pluginName}`;
