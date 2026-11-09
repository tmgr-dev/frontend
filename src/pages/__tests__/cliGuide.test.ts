import docsRoutes from '@/router/docsRoutes';
import { readdirSync, readFileSync, statSync } from 'fs';
import { join, relative } from 'path';
import { createMemoryHistory, createRouter } from 'vue-router';

const root = join(__dirname, '../../..');
const read = (rel: string) => readFileSync(join(root, rel), 'utf8');

const INSTALL_COMMAND = 'curl -fsSL https://tmgr.dev/install.sh | sh';

const walk = (dir: string): string[] =>
	readdirSync(dir).flatMap((name) => {
		const full = join(dir, name);
		return statSync(full).isDirectory() ? walk(full) : [full];
	});

const docs = walk(join(root, 'src/docs'))
	.filter((f) => f.endsWith('.md'))
	.map((f) => ({ file: relative(root, f), text: readFileSync(f, 'utf8') }));

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

const cliRs = read('crates/tmgr-cli/src/cli.rs');

const rustSources = (dir: string): string =>
	walk(join(root, dir))
		.filter((f) => f.endsWith('.rs'))
		.map((f) => readFileSync(f, 'utf8'))
		.join('\n');
const cliSources = rustSources('crates/tmgr-cli/src');

const GLOBAL_FLAGS = new Set<string>();
const GLOBAL_SHORT = new Map<string, string>();
for (const m of cliRs
	.split('pub struct Cli {')[1]
	.split('\n}')[0]
	.matchAll(/#\[arg\(([\s\S]*?)\)\]\s*pub (\w+):/g)) {
	if (!m[1].includes('global = true')) continue;
	GLOBAL_FLAGS.add(m[2]);
	const short = m[1].match(/\bshort(?: = '(\w)')?/);
	if (short) GLOBAL_SHORT.set(short[1] ?? m[2][0], m[2]);
}

const hasShort = (letter: string, field: string) =>
	new RegExp(
		`#\\[arg\\(short(?: = '${letter}')?[^\\n]*\\)\\]\\s*(?:pub )?${field}:`,
	).test(cliSources);

const tokenize = (cmd: string): string[] =>
	(cmd.match(/"[^"]*"|'[^']*'|\S+/g) ?? []).map((t) =>
		t.replace(/^(["'])([\s\S]*)\1$/, '$2'),
	);

const SEPARATORS = new Set(['|', '||', '&&', ';']);

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
	'ws use',
	'peek',
	'halt',
	'runner allow',
	'runner bind',
	'checkpoints list',
	'checkpoints diff',
	'checkpoints restore',
	'queue rules add',
	'skills show',
	'skills put',
	'skills attach',
	'agent start',
	'run',
	'rename',
	'send',
	'rm',
	'resume',
	'session summary',
	'session export',
	'runner trust',
	'runner untrust',
	'runner import',
	'page pull',
	'page status',
	'page push',
	'autopilot',
]);

function check(cmd: string): string[] {
	const problems: string[] = [];
	const tokens = tokenize(cmd);
	if (tokens[0] !== 'tmgr') return [`${cmd}: not a tmgr command`];
	const rest = tokens.slice(1);
	if (rest[0] === '--ws') rest.splice(0, 2);
	if (!rest.length) return [];
	const words: string[] = [];
	const flags: string[] = [];
	const shorts: string[] = [];
	for (const t of rest) {
		if (t.startsWith('--'))
			flags.push(t.slice(2).split('=')[0].replace(/-/g, '_'));
		else if (/^-[A-Za-z]$/.test(t)) shorts.push(t[1]);
		else if (flags.length === 0 && shorts.length === 0) words.push(t);
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
		const variant = words[0][0].toUpperCase() + words[0].slice(1);
		if (words.length === 1 && new RegExp(`\\n    ${variant}[,( {]`).test(cliRs))
			return [];
		return [`${cmd}: unknown command`];
	}
	const entry = snapshot.find((s) => s.name === name)!;
	const positional = words.slice(name.split(' ').length);
	if (positional.length && !POSITIONAL.has(name))
		problems.push(`${cmd}: "${name}" takes no positional argument`);
	const rootFlags = snapshot.find((s) => s.name === '<root>')!.flags;
	for (const f of flags) {
		if (
			![f, `${f}s`].some(
				(x) => entry.flags.has(x) || rootFlags.has(x) || GLOBAL_FLAGS.has(x),
			)
		)
			problems.push(`${cmd}: flag --${f} missing on "${name}"`);
	}
	for (const s of shorts) {
		const own = [...entry.flags].find((f) => f[0] === s && hasShort(s, f));
		if (!own && !GLOBAL_SHORT.has(s))
			problems.push(`${cmd}: flag -${s} missing on "${name}"`);
	}
	return problems;
}

const segments = (line: string): string[] => {
	const out: string[][] = [[]];
	for (const t of line.match(/"[^"]*"|'[^']*'|\S+/g) ?? []) {
		if (SEPARATORS.has(t)) out.push([]);
		else out[out.length - 1].push(t);
	}
	return out.filter((s) => s[0] === 'tmgr').map((s) => s.join(' '));
};

const fencedShell = (text: string): string[] =>
	[...text.matchAll(/^```(?:sh|bash|shell|zsh)\n([\s\S]*?)^```/gm)].flatMap(
		(m) =>
			m[1]
				.replace(/\\\n\s*/g, ' ')
				.split('\n')
				.map((l) => l.trim())
				.filter((l) => l && !l.startsWith('#')),
	);

const inlineTmgr = (text: string): string[] =>
	[
		...text.replace(/^```[\s\S]*?^```/gm, '').matchAll(/`(tmgr(?: [^`]*)?)`/g),
	].map((m) => m[1]);

const commandsIn = (text: string): string[] => [
	...fencedShell(text).flatMap(segments),
	...inlineTmgr(text),
];

describe('tmgr CLI docs', () => {
	it('finds the docs pages of both languages', () => {
		const files = docs.map((d) => d.file).sort();
		for (const lang of ['en', 'ru'])
			for (const slug of [
				'cli',
				'cli/install-and-sign-in',
				'cli/tasks-from-the-terminal',
			])
				expect(files).toContain(`src/docs/${lang}/${slug}.md`);
	});

	it.each(docs.map((d) => [d.file, d.text]))(
		'%s uses only commands and flags that exist in the CLI',
		(_file, text) => {
			expect(commandsIn(text).flatMap(check)).toEqual([]);
		},
	);

	it('checks a real number of commands on the CLI pages', () => {
		for (const lang of ['en', 'ru']) {
			const cli = docs.find((d) => d.file === `src/docs/${lang}/cli.md`)!;
			expect(commandsIn(cli.text).length).toBeGreaterThanOrEqual(30);
			const tutorial = docs.find(
				(d) => d.file === `src/docs/${lang}/cli/tasks-from-the-terminal.md`,
			)!;
			expect(commandsIn(tutorial.text)).toEqual(
				expect.arrayContaining([
					expect.stringMatching(/^tmgr add - /),
					expect.stringMatching(/^tmgr task list --json/),
				]),
			);
		}
	});

	it('rejects commands and flags the CLI does not have', () => {
		expect(check('tmgr workspace list')).not.toEqual([]);
		expect(check('tmgr board --nope')).not.toEqual([]);
		expect(check('tmgr task list -z')).not.toEqual([]);
		expect(check('tmgr status TM-123')).not.toEqual([]);
		expect(check('tmgr task list -p <project> --json -q')).toEqual([]);
		expect(check('tmgr')).toEqual([]);
		expect(
			segments(
				`tmgr task list --json | jq -r '.data.items[] | select(.x) | .ref'`,
			),
		).toEqual(['tmgr task list --json']);
		expect(segments('echo "a | b" | tmgr add - -p <project>')).toEqual([
			'tmgr add - -p <project>',
		]);
	});

	it('shows the install one-liner from the CLI readme', () => {
		expect(read('crates/README.md')).toContain(INSTALL_COMMAND);
		for (const lang of ['en', 'ru'])
			expect(fencedShell(read(`src/docs/${lang}/cli.md`))).toContain(
				INSTALL_COMMAND,
			);
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

	it('sends /cli to the public docs page, keeping hash and query', async () => {
		const Stub = { render: () => null };
		const router = createRouter({
			history: createMemoryHistory(),
			routes: docsRoutes.map((r: any) =>
				r.component ? { ...r, component: Stub } : r,
			),
		});
		await router.push('/cli?x=1#tasks');
		expect(router.currentRoute.value.fullPath).toBe('/docs/cli?x=1#tasks');
		expect(router.currentRoute.value.meta).toMatchObject({
			allowedGuests: true,
			notOnlyForLoggedUsers: true,
		});
	});

	it('links to the page from Settings and the public footer', () => {
		const settings = read('src/pages/Settings.vue').replace(/\s+/g, ' ');
		expect(settings).toContain('href="https://tmgr.dev/cli" target="_blank"');
		expect(settings).toContain("$t('settings.about.cliLink')");
		const footer = read('src/components/privacy/PrivacyLinks.vue');
		expect(footer).toContain('to="/docs/cli"');
		expect(footer).toContain('to="/docs"');
	});

	it('links to the agents guide and @tmgr/notify', () => {
		for (const lang of ['en', 'ru']) {
			const page = read(`src/docs/${lang}/cli.md`);
			expect(page).toMatch(/\]\(\/docs(\/ru)?\/agents\)/);
			expect(page).toContain('npmjs.com/package/@tmgr/notify');
		}
	});
});
