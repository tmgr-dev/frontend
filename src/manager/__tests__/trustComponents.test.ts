import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createSSRApp, type Component } from 'vue';
import { createI18n } from 'vue-i18n';
import { renderToString } from 'vue/server-renderer';
import manager from '../../locales/en/manager.json';
import type { TrustDevice } from '../types';

const dir = join(__dirname, '..', 'components', 'trust');
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
	if (id === './trustHelpers')
		return require('../components/trust/trustHelpers');
	return require(id);
};

const i18n = createI18n({
	legacy: true,
	locale: 'en',
	messages: { en: { manager } },
});

const render = (name: string, props: Record<string, unknown>, slots = {}) =>
	renderToString(
		createSSRApp({
			render: () => require('vue').h(load(name), props, slots),
		}).use(i18n),
	);

const devices: TrustDevice[] = [
	{
		id: 'a',
		kind: 'desktop',
		label: 'Mac',
		fp: 'ab12cd34ef56ab12ff',
		state: 'trusted',
		is_self: true,
	},
	{
		id: 'b',
		kind: 'mobile',
		label: 'Phone',
		fp: '1111222233334444',
		state: 'trusted',
		is_self: false,
	},
	{
		id: 'c',
		kind: 'runner',
		label: 'Old box',
		fp: '9999888877776666',
		state: 'revoked',
		is_self: false,
	},
];

describe('trust components', () => {
	it('JoinConfirmDialog shows kind, label, grouped fp and both actions', async () => {
		const html = await render('JoinConfirmDialog', {
			label: 'iPhone',
			kind: 'mobile',
			fp: 'ab12cd34ef56ab12ff',
		});
		expect(html).toContain('role="dialog"');
		expect(html).toContain('aria-modal="true"');
		expect(html).toContain('wants to read your chats');
		expect(html).toContain('iPhone');
		expect(html).toContain('mobile');
		expect(html).toContain('ab12 cd34 ef56 ab12');
		expect(html).not.toContain('ab12 cd34 ef56 ab12 ff');
		expect(html).toContain('Approve');
		expect(html).toContain('Reject');
		expect(html).toContain('Compare this fingerprint');
	});

	it('RevokeConfirmDialog shows revoker, target and consequences', async () => {
		const html = await render('RevokeConfirmDialog', {
			revokerLabel: 'Phone',
			revokerFp: '1111222233334444',
			targetLabel: 'Laptop',
			targetFp: 'ee00ff11ee00ff11',
		});
		expect(html).toContain('Phone wants to revoke Laptop');
		expect(html).toContain('1111 2222 3333 4444');
		expect(html).toContain('ee00 ff11 ee00 ff11');
		expect(html).toContain('stop receiving your chats');
		expect(html).toContain('only introduced by it');
	});

	it('RevokeConfirmDialog without a revoker is the outgoing confirm', async () => {
		const html = await render('RevokeConfirmDialog', {
			targetLabel: 'Laptop',
			targetFp: 'ee00ff11ee00ff11',
		});
		expect(html).toContain('Revoke this device?');
		expect(html).not.toContain('Revoked by');
	});

	it('ResetIdentityDialog disables the button until RESET is typed', async () => {
		const off = await render('ResetIdentityDialog', { modelValue: 'rese' });
		expect(off).toContain('Type RESET to confirm');
		expect(off).toMatch(/<button[^>]*disabled[^>]*>\s*Reset identity/);
		const on = await render('ResetIdentityDialog', { modelValue: 'RESET' });
		expect(on).not.toMatch(/<button[^>]*disabled[^>]*>\s*Reset identity/);
	});

	it('DevicesPanel lists devices with pills; revoke only on other live devices', async () => {
		const html = await render('DevicesPanel', { devices });
		expect(html).toContain('this device');
		expect(html).toContain('Trusted');
		expect(html).toContain('Revoked');
		expect(html).toContain('Revoke Phone');
		expect(html).not.toContain('Revoke Mac');
		expect(html).not.toContain('Revoke Old box');
		expect(html).toContain('Reset chat identity');
	});

	it('DevicesPanel offers a kind confirmation only on rows that allow it', async () => {
		const rows: TrustDevice[] = [
			devices[0],
			{ ...devices[1], can_confirm_kind: true },
			{ ...devices[2], state: 'trusted' },
		];
		const html = await render('DevicesPanel', { devices: rows });
		expect(html.match(/Confirm as desktop/g)).toHaveLength(1);
		expect(html.match(/Confirm as phone/g)).toHaveLength(1);
		expect(html).toContain('Confirm Phone as desktop');
		expect(html).toContain('Confirm Phone as phone');
		const none = await render('DevicesPanel', { devices });
		expect(none).not.toContain('Confirm as');
	});

	it('ConfirmKindDialog shows the device label, kind and grouped fingerprint', async () => {
		const html = await render('ConfirmKindDialog', {
			label: 'Phone',
			kind: 'mobile',
			fp: '1111222233334444ff',
			error: 'this device kind cannot be confirmed',
		});
		expect(html).toContain('role="dialog"');
		expect(html).toContain('Confirm this device kind?');
		expect(html).toContain('Phone');
		expect(html).toContain('1111 2222 3333 4444');
		expect(html).toContain('this device kind cannot be confirmed');
		expect(html).toContain('Cancel');
	});

	it('DevicesPanel renders every alert kind once and gaps per sender', async () => {
		const alerts = [
			{ kind: 'room_conflict', device_id: 'd1', at_ms: 1 },
			{ kind: 'kind_conflict', device_id: 'd2', at_ms: 2 },
			{ kind: 'kind_conflict', device_id: 'd3', at_ms: 3 },
			{ kind: 'join_code_reused', device_id: 'd9', at_ms: 4 },
			{ kind: 'weird', device_id: 'd9', at_ms: 5 },
			{ kind: 'root_kind_unknown', device_id: 'd7', at_ms: 6 },
			{ kind: 'kind_unknown', device_id: 'd8', at_ms: 7 },
			{ kind: 'trust_unstable', device_id: 'd8', at_ms: 8 },
		];
		const seqGaps = [
			{ from_device: devices[1].id, expected: 3, got: 7 },
			{ from_device: devices[1].id, expected: 10, got: 11 },
		];
		const html = await render('DevicesPanel', { devices, alerts, seqGaps });
		expect(html).toContain('claimed a chat room');
		expect(html.match(/disagree about what kind of device/g)).toHaveLength(1);
		expect(html).toContain('already used, closed or expired');
		expect(html).toContain('security alert (weird)');
		expect(html).toContain('Re-join this device with a fresh join code');
		expect(html.match(/has no confirmed kind/g)).toHaveLength(1);
		expect(
			html.match(/kept changing while it was being checked/g),
		).toHaveLength(1);
		expect(html).toContain(
			`5 messages may be missing from ${devices[1].label}`,
		);
		const none = await render('DevicesPanel', { devices });
		expect(none).not.toContain('may be missing');
		expect(none).not.toContain('role="alert"');
	});

	it('DevicesPanel shows a one-line alert when a spent join code was reused', async () => {
		const alerts = [{ kind: 'join_code_reused', device_id: 'd9', at_ms: 1 }];
		const on = await render('DevicesPanel', { devices, alerts });
		expect(on).toContain('already used, closed or expired');
		expect(on).toContain('role="alert"');
		const off = await render('DevicesPanel', { devices });
		expect(off).not.toContain('already used, closed or expired');
	});

	it('PairDeviceDialog shows the QR, the code and the countdown', async () => {
		const html = await render('PairDeviceDialog', {
			mode: 'show',
			code: 'tmgr-chat-join:v2:abc',
			qr: 'data:image/svg+xml;base64,AAAA',
			secondsLeft: 125,
		});
		expect(html).toContain('role="dialog"');
		expect(html).toContain('src="data:image/svg+xml;base64,AAAA"');
		expect(html).toContain('value="tmgr-chat-join:v2:abc"');
		expect(html).toContain('readonly');
		expect(html).toContain('Expires in 2:05');
		expect(html).toContain('Copy code');
	});

	it('PairDeviceDialog shows no code once expired and offers a new one', async () => {
		const html = await render('PairDeviceDialog', {
			mode: 'show',
			code: '',
			qr: '',
			expired: true,
		});
		expect(html).not.toContain('<img');
		expect(html).toContain('The code expired');
		expect(html).toContain('New code');
	});

	it('PairDeviceDialog enter mode has a paste field, Join and the error', async () => {
		const html = await render('PairDeviceDialog', {
			mode: 'enter',
			modelValue: '',
			error: 'the join code is not valid',
		});
		expect(html).toContain('Join code');
		expect(html).toContain('the join code is not valid');
		expect(html).toMatch(/<button[^>]*disabled[^>]*>\s*Join\s*<\/button>/);
	});

	it('DevicesPanel offers Add device and Enter code from phone', async () => {
		const html = await render('DevicesPanel', { devices });
		expect(html).toContain('Add device');
		expect(html).toContain('Enter code from phone');
	});

	it('DevicesPanel hides the paste button once the device trusts others', async () => {
		const html = await render('DevicesPanel', { devices, canJoin: false });
		expect(html).not.toContain('Enter code from phone');
		expect(html).toContain('already trusts other devices');
	});

	it('DevicesPanel says it is waiting and offers a different code', async () => {
		const html = await render('DevicesPanel', { devices, joining: true });
		expect(html).toContain('Waiting for the other device');
		expect(html).toContain('Use a different code');
		expect(html).toMatch(
			/<button[^>]*disabled[^>]*>\s*Add device\s*<\/button>/,
		);
	});

	it('DevicesPanel shows the automatic runner setup error', async () => {
		const html = await render('DevicesPanel', {
			devices,
			autoJoinError: 'join refused',
		});
		expect(html).toContain(
			'Automatic setup of the local runner failed: join refused',
		);
		const quiet = await render('DevicesPanel', { devices });
		expect(quiet).not.toContain('Automatic setup');
	});
});
