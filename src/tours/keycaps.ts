import { t } from '@/i18n';
import type { StepDef } from './types';

export type BodyPart =
	{ kind: 'text'; text: string } | { kind: 'key'; text: string };

export interface KeycapEnv {
	mac: boolean;
	shortcut(action: string): { accelerator: string; enabled: boolean } | null;
	describe(accelerator: string): string;
}

export const DEFAULT_KEYCAP_ENV: KeycapEnv = {
	mac: false,
	shortcut: () => null,
	describe: (accelerator) => accelerator,
};

const SHORTCUT_LABEL_KEYS = ['quickAdd', 'timer', 'screenshot', 'selection'];

const TOKEN = /\{(key:[A-Za-z]+)\}/g;

export const templateFor = (
	step: StepDef,
	env: Pick<KeycapEnv, 'mac'>,
): string => {
	const variants = step.bodyByPlatform;
	if (!variants) return step.body;
	return (env.mac ? variants.macos : variants.other) ?? step.body;
};

export const tokensIn = (template: string): string[] =>
	[...template.matchAll(TOKEN)].map((match) => match[1]);

export const hasTemplate = (step: StepDef): boolean =>
	!!step.bodyByPlatform || tokensIn(step.body).length > 0;

export const fallbackFor = (action: string): string =>
	t('onboarding.tours.keycaps.fallback', {
		name: SHORTCUT_LABEL_KEYS.includes(action)
			? t(`onboarding.tours.keycaps.shortcuts.${action}`)
			: action,
	});

export function resolveBodyParts(step: StepDef, env: KeycapEnv): BodyPart[] {
	const template = templateFor(step, env);
	const parts: BodyPart[] = [];
	const pushText = (text: string) => {
		if (!text) return;
		const last = parts[parts.length - 1];
		if (last?.kind === 'text') last.text += text;
		else parts.push({ kind: 'text', text });
	};
	let cursor = 0;
	for (const match of template.matchAll(TOKEN)) {
		pushText(template.slice(cursor, match.index));
		cursor = (match.index ?? 0) + match[0].length;
		const token = match[1];
		const action = token.slice(4);
		const setting = env.shortcut(action);
		if (setting?.enabled && setting.accelerator) {
			parts.push({ kind: 'key', text: env.describe(setting.accelerator) });
		} else pushText(fallbackFor(action));
	}
	pushText(template.slice(cursor));
	return parts;
}

export const partsToText = (parts: BodyPart[]): string =>
	parts.map((part) => part.text).join('');
