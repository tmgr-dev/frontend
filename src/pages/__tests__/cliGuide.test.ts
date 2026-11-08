import { readFileSync } from 'fs';
import { join } from 'path';
import {
	CLI_EXTRA_COMMANDS,
	CLI_SCENARIOS,
	INSTALL_COMMAND,
} from '../cliGuideContent';

const root = join(__dirname, '../../..');
const read = (rel: string) => readFileSync(join(root, rel), 'utf8');

const snapshot = read(
	'crates/tmgr-cli/src/cmd/ui/snapshots/commands_runner.txt',
)
	.split('\n')
	.filter(Boolean)
	.map((line) => {
		const [name, , flags] = line.split(' | ');
		return {
			name: name.trim(),
			flags: new Set(
				flags.trim() === '-' ? [] : flags.split(',').map((f) => f.trim()),
			),
		};
	});

const tokenize = (cmd: string): string[] =>
	(cmd.match(/"[^"]*"|\S+/g) ?? []).map((t) => t.replace(/^"|"$/g, ''));

const POSITIONAL = new Set([
	'add',
	'notify',
	'alarm',
	'start',
	'switch',
	'task show',
	'usage task',
	'hook install',
	'init',
]);

function check(cmd: string): string[] {
	const problems: string[] = [];
	const tokens = tokenize(cmd);
	if (tokens[0] !== 'tmgr') return [`${cmd}: not a tmgr command`];
	const rest = tokens.slice(1);
	if (rest[0] === '--ws') rest.splice(0, 2);
	const words: string[] = [];
	const flags: string[] = [];
	for (let i = 0; i < rest.length; i++) {
		const t = rest[i];
		if (t.startsWith('--')) flags.push(t.slice(2).replace(/-/g, '_'));
		else if (flags.length === 0) words.push(t);
	}
	const names = new Set(snapshot.map((s) => s.name));
	let name = '';
	for (let n = words.length; n > 0; n--) {
		const candidate = words.slice(0, n).join(' ');
		if (names.has(candidate)) {
			name = candidate;
			break;
		}
	}
	if (!name) {
		if (CLI_EXTRA_COMMANDS.includes(words[0])) {
			const cli = read('crates/tmgr-cli/src/cli.rs');
			const variant = words[0][0].toUpperCase() + words[0].slice(1);
			if (!new RegExp(`\\n    ${variant}[,( {]`).test(cli))
				problems.push(`${cmd}: ${words[0]} is not in cli.rs`);
			return problems;
		}
		return [`${cmd}: unknown command`];
	}
	const entry = snapshot.find((s) => s.name === name)!;
	const positional = words.slice(name.split(' ').length);
	if (positional.length && !POSITIONAL.has(name))
		problems.push(`${cmd}: "${name}" takes no positional argument`);
	const rootFlags = snapshot.find((s) => s.name === '<root>')!.flags;
	for (const f of flags) {
		if (![f, `${f}s`].some((x) => entry.flags.has(x) || rootFlags.has(x)))
			problems.push(`${cmd}: flag --${f} missing on "${name}"`);
	}
	return problems;
}

describe('tmgr CLI explainer', () => {
	it('uses only commands and flags that exist in the CLI', () => {
		const all = CLI_SCENARIOS.flatMap((s) => [s.example, ...s.also]);
		expect(all.flatMap(check)).toEqual([]);
	});

	it('has an example for every scenario', () => {
		for (const s of CLI_SCENARIOS) {
			expect(s.example.startsWith('tmgr ')).toBe(true);
			expect(s.benefit.length).toBeGreaterThan(40);
		}
	});

	it('shows the install one-liner from the CLI readme', () => {
		expect(INSTALL_COMMAND).toBe('curl -fsSL https://tmgr.dev/install.sh | sh');
		expect(read('crates/README.md')).toContain(INSTALL_COMMAND);
	});

	it('keeps README Why commands real', () => {
		const readme = read('crates/README.md');
		const why = readme.split('## Why tmgr')[1].split('\n## ')[0];
		const fenced = why
			.split('\n')
			.map((l) => l.trim())
			.filter((l) => l.startsWith('tmgr '));
		const inline = [...why.matchAll(/`(tmgr [^`]+)`/g)].map((m) => m[1]);
		const cmds = [...fenced, ...inline];
		expect(cmds.length).toBeGreaterThanOrEqual(7);
		expect(cmds.flatMap(check)).toEqual([]);
	});

	it('registers /cli as a public route', () => {
		const routes = read('src/router/routes.js').replace(/\s+/g, ' ');
		expect(routes).toMatch(
			/path: '\/cli', component: \(\) => import\('@\/pages\/CliGuide.vue'\), meta: \{[^}]*allowedGuests: true, notOnlyForLoggedUsers: true/,
		);
	});

	it('links to the page from Settings and the public footer', () => {
		const settings = read('src/pages/Settings.vue').replace(/\s+/g, ' ');
		expect(settings).toContain('href="https://tmgr.dev/cli" target="_blank"');
		expect(settings).toContain("$t('settings.about.cliLink')");
		expect(read('src/components/privacy/PrivacyLinks.vue')).toContain(
			'to="/cli"',
		);
	});

	it('links to the agents guide and @tmgr/notify', () => {
		const page = read('src/pages/CliGuide.vue');
		expect(page).toContain('to="/docs/agents"');
		expect(page).toContain('npmjs.com/package/@tmgr/notify');
	});
});
