import { describeAccelerator, mergeShortcuts } from '@/utils/desktopShortcuts';
import type { KeycapEnv } from '../keycaps';
import { partsToText, resolveBodyParts } from '../keycaps';
import type { StepDef } from '../types';

const step = (over: Partial<StepDef>): StepDef => ({
	id: 's1',
	type: 'modal',
	title: 't',
	body: 'b',
	...over,
});

const env = (
	platform: 'macos' | 'windows' | 'linux',
	stored = {},
): KeycapEnv => {
	const config = mergeShortcuts(stored);
	return {
		mac: platform === 'macos',
		tray: platform === 'macos' ? 'menu bar' : 'system tray',
		shortcut: (action) => config[action as 'quickAdd'] ?? null,
		describe: (accelerator) => describeAccelerator(accelerator, platform),
	};
};

describe('keycaps', () => {
	const quick = step({ body: 'Press {key:quickAdd} in any app.' });

	it('renders the macOS accelerator as a key', () => {
		expect(resolveBodyParts(quick, env('macos'))).toEqual([
			{ kind: 'text', text: 'Press ' },
			{ kind: 'key', text: '⌥Space' },
			{ kind: 'text', text: ' in any app.' },
		]);
	});

	it('renders Alt+Space on Windows and Linux', () => {
		for (const platform of ['windows', 'linux'] as const)
			expect(partsToText(resolveBodyParts(quick, env(platform)))).toBe(
				'Press Alt+Space in any app.',
			);
	});

	it('follows a remapped accelerator', () => {
		const parts = resolveBodyParts(
			quick,
			env('windows', { quickAdd: { accelerator: 'CommandOrControl+Alt+Q' } }),
		);
		expect(parts.find((p) => p.kind === 'key')?.text).toBe('Ctrl+Alt+Q');
	});

	it('falls back to words when the shortcut is off', () => {
		const parts = resolveBodyParts(
			quick,
			env('macos', { quickAdd: { enabled: false } }),
		);
		expect(parts.every((p) => p.kind === 'text')).toBe(true);
		expect(partsToText(parts)).toBe(
			'Press your quick-add shortcut (Settings → Desktop) in any app.',
		);
	});

	it('falls back when the host cannot read shortcuts', () => {
		const bare: KeycapEnv = { ...env('macos'), shortcut: () => null };
		expect(partsToText(resolveBodyParts(quick, bare))).toContain(
			'(Settings → Desktop)',
		);
	});

	it('resolves {tray} per platform', () => {
		const s = step({ body: 'Stays in the {tray}.' });
		expect(partsToText(resolveBodyParts(s, env('macos')))).toBe(
			'Stays in the menu bar.',
		);
		expect(partsToText(resolveBodyParts(s, env('linux')))).toBe(
			'Stays in the system tray.',
		);
	});

	it('picks the platform variant', () => {
		const s = step({
			body: 'default',
			bodyByPlatform: { macos: 'mac text', other: 'other text' },
		});
		expect(partsToText(resolveBodyParts(s, env('macos')))).toBe('mac text');
		expect(partsToText(resolveBodyParts(s, env('windows')))).toBe('other text');
		expect(
			partsToText(
				resolveBodyParts(
					step({ body: 'd', bodyByPlatform: { macos: 'm' } }),
					env('linux'),
				),
			),
		).toBe('d');
	});
});
