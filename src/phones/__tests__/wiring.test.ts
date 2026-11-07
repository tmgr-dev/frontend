import { parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';

const root = join(__dirname, '../../..');
const read = (rel: string) => readFileSync(join(root, rel), 'utf8');
const capability = (name: string) =>
	JSON.parse(read(`src-tauri/capabilities/${name}.json`));

const WINDOW_COMMANDS = ['list', 'pairing', 'confirm', 'cancel', 'revoke'];

describe('phones window wiring', () => {
	it('declares every command in the app manifest', () => {
		const build = read('src-tauri/build.rs');
		for (const c of ['begin_pairing', 'open_window', ...WINDOW_COMMANDS]) {
			expect(build).toContain(`"phones_${c}",`);
		}
	});

	it('gives the phones window only its own commands', () => {
		const cap = capability('decisions-phones');
		expect(cap.windows).toEqual(['decisions-phones']);
		const own = cap.permissions.filter((p: string) => p.startsWith('allow-'));
		expect(own.sort()).toEqual(
			WINDOW_COMMANDS.map((c) => `allow-phones-${c}`).sort(),
		);
		expect(cap.permissions).toEqual(
			expect.arrayContaining([
				'core:event:allow-listen',
				'core:event:allow-unlisten',
			]),
		);
		expect(cap.permissions.some((p: string) => /decisions/.test(p))).toBe(
			false,
		);
	});

	it('gives begin and open only to the main-side default capability', () => {
		const perms = capability('default').permissions;
		expect(perms).toContain('allow-phones-begin-pairing');
		expect(perms).toContain('allow-phones-open-window');
		for (const c of WINDOW_COMMANDS) {
			expect(perms).not.toContain(`allow-phones-${c}`);
		}
		expect(capability('decisions').permissions.join()).not.toMatch(/phones/);
		expect(capability('decisions-phones').permissions).not.toContain(
			'allow-phones-begin-pairing',
		);
	});

	it('keeps the strict CSP and registers the page in vite', () => {
		const html = read('decisions-phones.html');
		expect(html).toContain("img-src 'self' data:");
		expect(html).toContain('connect-src ipc: http://ipc.localhost;');
		expect(html).not.toMatch(/https?:\/\/[^"' ]*\.(com|dev)/);
		const vite = read('vite.config.ts');
		expect(vite).toContain("'/decisions-phones.html'");
		expect(vite).toContain("'decisions-phones': fileURLToPath(");
	});

	it('never renders card text in the phones window', () => {
		const src = read('src/decisions-phones/App.vue');
		expect(src).not.toMatch(/decisions\/|v-html|CardView/);
	});

	it('gates the settings section on desktop with an account', () => {
		const { descriptor } = parse(read('src/pages/Settings/PhonesSection.vue'));
		const rootEl = descriptor.template!.ast!.children.find(
			(n: any) => n.type === 1,
		) as any;
		expect(rootEl.props.some((p: any) => p.name === 'if')).toBe(true);
		expect(descriptor.scriptSetup ?? descriptor.script!.content).toContain(
			'phonesAvailable(isDesktopApp(), store.getters.isLocalOnly)',
		);
		expect(descriptor.template!.content).toContain('Pair an iPhone');
	});
});
