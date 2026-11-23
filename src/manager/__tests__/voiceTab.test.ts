/**
 * @jest-environment jsdom
 */
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createApp, h, nextTick, type Component } from 'vue';
import { createI18n } from 'vue-i18n';
import manager from '../../locales/en/manager.json';
import { externalVoice } from '../tts/externalVoice';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));

const dir = join(__dirname, '..', 'components');
const cache = new Map<string, Component>();

const toCjs = (code: string): string =>
	ts.transpileModule(code, {
		compilerOptions: {
			module: ts.ModuleKind.CommonJS,
			target: ts.ScriptTarget.ES2020,
			esModuleInterop: true,
		},
	}).outputText;

const run = (code: string, requireFn: (id: string) => unknown) => {
	const module = { exports: {} as Record<string, unknown> };
	new Function('module', 'exports', 'require', code)(
		module,
		module.exports,
		requireFn,
	);
	return module.exports;
};

const load = (name: string): Component => {
	const hit = cache.get(name);
	if (hit) return hit;
	const filename = join(dir, `${name}.vue`);
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const script = compileScript(descriptor, { id: name });
	const scriptExports = run(toCjs(script.content), localRequire);
	const comp = (scriptExports.default ?? scriptExports) as Component & {
		render?: unknown;
	};
	const tpl = compileTemplate({
		source: descriptor.template!.content,
		filename,
		id: name,
		compilerOptions: { mode: 'module' },
	});
	const tplExports = run(toCjs(tpl.code), localRequire);
	comp.render = tplExports.render;
	cache.set(name, comp);
	return comp;
};

const localRequire = (id: string): unknown => {
	if (id.endsWith('.vue'))
		return load(id.replace(/^.*\//, '').replace('.vue', ''));
	if (id === './viewHelpers') return require('../components/viewHelpers');
	if (id === './trust/trustHelpers')
		return require('../components/trust/trustHelpers');
	if (id === '../types') return require('../types');
	return require(id);
};

const i18n = createI18n({
	legacy: true,
	locale: 'en',
	messages: { en: { manager } },
});

const mountDom = () => {
	const el = document.createElement('div');
	document.body.appendChild(el);
	const app = createApp({ render: () => h(load('VoiceTab')) }).use(i18n);
	app.mount(el);
	return {
		el,
		unmount: () => {
			app.unmount();
			el.remove();
		},
	};
};

const flush = async () => {
	await new Promise((resolve) => setTimeout(resolve, 0));
	await nextTick();
};

const settings = (mode: 'off' | 'http' | 'command') => ({
	mode,
	http: {
		baseUrl: 'http://127.0.0.1:8000/v1',
		model: 'silero',
		voice: 'xenia',
		allowRemote: false,
	},
	command: { argv: ['/usr/bin/piper', '--fast'], timeoutSecs: 20 },
});

const health = (over: Record<string, unknown> = {}) => ({
	configured: true,
	mode: 'http',
	healthy: true,
	lastError: null,
	...over,
});

const { invoke } = require('@tauri-apps/api/core') as { invoke: jest.Mock };

const backend = (
	initial: ReturnType<typeof settings>,
	over: Record<string, unknown> = {},
) => {
	invoke.mockImplementation(
		async (cmd: string, args: { settings?: unknown }) => {
			if (cmd === 'tts_settings_get') return initial;
			if (cmd === 'tts_settings_set') return args.settings;
			if (cmd === 'tts_external_health') return health(over);
			return undefined;
		},
	);
};

const q = <T extends Element>(el: HTMLElement, sel: string) =>
	el.querySelector<T>(sel);
const type = async (
	input: HTMLInputElement | HTMLSelectElement,
	value: string,
) => {
	input.value = value;
	input.dispatchEvent(
		new Event(input instanceof HTMLSelectElement ? 'change' : 'input'),
	);
	await nextTick();
};
const click = async (btn: Element | null) => {
	(btn as HTMLElement).click();
	await flush();
};

describe('VoiceTab', () => {
	beforeEach(() => {
		invoke.mockReset();
		externalVoice.saveError = null;
		externalVoice.testError = null;
		externalVoice.testing = false;
		externalVoice.health = null;
	});

	it('shows no fields when the mode is off', async () => {
		backend(settings('off'), { mode: 'off', configured: false });
		const m = mountDom();
		await flush();
		expect(q<HTMLSelectElement>(m.el, '.vt-mode')?.value).toBe('off');
		expect(q(m.el, '.vt-url')).toBeNull();
		expect(q(m.el, '.vt-arg')).toBeNull();
		expect(m.el.textContent).toContain('Off');
		m.unmount();
	});

	it('shows the server fields and the remote warning in http mode', async () => {
		backend(settings('http'));
		const m = mountDom();
		await flush();
		expect(q<HTMLInputElement>(m.el, '.vt-url')?.value).toBe(
			'http://127.0.0.1:8000/v1',
		);
		expect(q<HTMLInputElement>(m.el, '.vt-model')?.value).toBe('silero');
		expect(q<HTMLInputElement>(m.el, '.vt-voice')?.value).toBe('xenia');
		expect(q<HTMLInputElement>(m.el, '.vt-remote')?.checked).toBe(false);
		expect(m.el.textContent).toContain('Allow non-local server');
		expect(m.el.textContent).toContain('Chat text will be sent to this server');
		expect(q(m.el, '.vt-arg')).toBeNull();
		expect(m.el.textContent).toContain('Ready');
		m.unmount();
	});

	it('shows one input per argument in command mode with the stdin note', async () => {
		backend(settings('command'), { mode: 'command' });
		const m = mountDom();
		await flush();
		const args = m.el.querySelectorAll<HTMLInputElement>('.vt-arg');
		expect(Array.from(args).map((a) => a.value)).toEqual([
			'/usr/bin/piper',
			'--fast',
		]);
		expect(q<HTMLInputElement>(m.el, '.vt-timeout')?.value).toBe('20');
		expect(m.el.textContent).toContain(
			'The text is passed on standard input; the program must write a WAV file to standard output.',
		);
		expect(q(m.el, '.vt-url')).toBeNull();
		m.unmount();
	});

	it('adds and removes arguments and saves them as an array', async () => {
		backend(settings('command'), { mode: 'command' });
		const m = mountDom();
		await flush();
		await click(q(m.el, '.vt-add'));
		const args = m.el.querySelectorAll<HTMLInputElement>('.vt-arg');
		await type(args[2], '--out');
		await click(m.el.querySelectorAll('.vt-remove')[1]);
		await click(q(m.el, '.vt-save'));
		expect(invoke).toHaveBeenCalledWith('tts_settings_set', {
			settings: expect.objectContaining({
				mode: 'command',
				command: { argv: ['/usr/bin/piper', '--out'], timeoutSecs: 20 },
			}),
		});
		m.unmount();
	});

	it('saves the edited server settings', async () => {
		backend(settings('http'));
		const m = mountDom();
		await flush();
		await type(q<HTMLInputElement>(m.el, '.vt-model')!, 'other');
		await click(q(m.el, '.vt-remote'));
		await click(q(m.el, '.vt-save'));
		expect(invoke).toHaveBeenCalledWith('tts_settings_set', {
			settings: expect.objectContaining({
				mode: 'http',
				http: expect.objectContaining({ model: 'other', allowRemote: true }),
			}),
		});
		m.unmount();
	});

	it('switching the mode reveals the matching fields', async () => {
		backend(settings('off'), { mode: 'off', configured: false });
		const m = mountDom();
		await flush();
		await type(q<HTMLSelectElement>(m.el, '.vt-mode')!, 'http');
		expect(q(m.el, '.vt-url')).not.toBeNull();
		await type(q<HTMLSelectElement>(m.el, '.vt-mode')!, 'command');
		expect(q(m.el, '.vt-url')).toBeNull();
		expect(q(m.el, '.vt-add')).not.toBeNull();
		m.unmount();
	});

	it('shows nothing when the native confirmation is cancelled', async () => {
		backend(settings('http'));
		const m = mountDom();
		await flush();
		invoke.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_settings_set') throw 'cancelled';
			return health();
		});
		await click(q(m.el, '.vt-save'));
		expect(q(m.el, '.vt-save-error')).toBeNull();
		m.unmount();
	});

	it('shows a save error', async () => {
		backend(settings('http'));
		const m = mountDom();
		await flush();
		invoke.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_settings_set') throw 'invalid base url';
			return health();
		});
		await click(q(m.el, '.vt-save'));
		expect(q(m.el, '.vt-save-error')?.textContent).toContain(
			'invalid base url',
		);
		m.unmount();
	});

	it('Test voice shows the backend error and the refreshed last error', async () => {
		backend(settings('http'));
		const m = mountDom();
		await flush();
		invoke.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_external_test') throw 'connection refused';
			return health({ healthy: false, lastError: 'connection refused' });
		});
		await click(q(m.el, '.vt-test'));
		expect(q(m.el, '.vt-test-error')?.textContent).toContain(
			'connection refused',
		);
		expect(q(m.el, '.vt-last-error')?.textContent).toContain(
			'Last error: connection refused',
		);
		expect(m.el.textContent).toContain('Not working');
		m.unmount();
	});

	it('Test voice plays the returned audio', async () => {
		backend(settings('http'));
		const m = mountDom();
		await flush();
		const created: number[] = [];
		const g = globalThis as unknown as Record<string, unknown>;
		g.AudioContext = class {
			state = 'running';
			currentTime = 0;
			destination = {};
			constructor() {
				created.push(1);
			}
			createBufferSource() {
				return {
					connect: jest.fn(),
					start: jest.fn(),
					stop: jest.fn(),
					onended: null,
				};
			}
			createBuffer(_c: number, length: number, rate: number) {
				return {
					duration: length / rate,
					getChannelData: () => new Float32Array(length),
				};
			}
		};
		const buf = new ArrayBuffer(8);
		new DataView(buf).setUint32(0, 16000, true);
		invoke.mockImplementation(async (cmd: string) =>
			cmd === 'tts_external_test' ? buf : health(),
		);
		await click(q(m.el, '.vt-test'));
		expect(created).toHaveLength(1);
		expect(q(m.el, '.vt-test-error')).toBeNull();
		delete g.AudioContext;
		m.unmount();
	});

	it('shows the Silero hint inline without fetching anything', async () => {
		backend(settings('http'));
		const m = mountDom();
		await flush();
		expect(q(m.el, '.vt-hint')).toBeNull();
		await click(q(m.el, '.vt-silero'));
		expect(q(m.el, '.vt-hint')?.textContent).toContain('docs/tts-external.md');
		m.unmount();
	});
});
