import {
	TOKEN_PLACEHOLDER,
	buildNotifySnippets,
	resolveNotifyApiUrl,
	shellQuote,
	tomlString,
} from '../agentConnectSnippets';

const TOKEN = 'tmgrn_EXAMPLE0000000000';

describe('buildNotifySnippets', () => {
	it('uses the placeholder when there is no token', () => {
		for (const token of [undefined, null, '']) {
			const snippets = buildNotifySnippets({ token });

			expect(snippets.claudeCode).toContain(TOKEN_PLACEHOLDER);
			expect(snippets.codex).toContain(TOKEN_PLACEHOLDER);
			expect(snippets.mcpJson).toContain(TOKEN_PLACEHOLDER);
			expect(snippets.hooksEnvFile).toContain(TOKEN_PLACEHOLDER);
		}
	});

	it('prefills the token in every token-bearing snippet', () => {
		const snippets = buildNotifySnippets({ token: TOKEN });

		expect(snippets.claudeCode).toContain(`-e 'TMGR_NOTIFY_TOKEN=${TOKEN}'`);
		expect(snippets.codex).toContain(`TMGR_NOTIFY_TOKEN = "${TOKEN}"`);
		expect(JSON.parse(snippets.mcpJson).mcpServers['tmgr-notify'].env).toEqual({
			TMGR_NOTIFY_TOKEN: TOKEN,
		});
		expect(snippets.hooksEnvFile).toContain(`'TMGR_NOTIFY_TOKEN=${TOKEN}'`);
		expect(snippets.claudeCode).not.toContain(TOKEN_PLACEHOLDER);
	});

	it('quotes the placeholder in shell snippets', () => {
		const snippets = buildNotifySnippets({});

		expect(snippets.claudeCode).toContain(
			`-e 'TMGR_NOTIFY_TOKEN=${TOKEN_PLACEHOLDER}'`,
		);
		expect(snippets.hooksEnvFile).toContain(
			`'TMGR_NOTIFY_TOKEN=${TOKEN_PLACEHOLDER}'`,
		);
	});

	it('omits TMGR_URL without an api url and adds it with one', () => {
		const without = buildNotifySnippets({ token: TOKEN, apiUrl: null });
		expect(without.claudeCode).not.toContain('TMGR_URL');
		expect(without.codex).not.toContain('TMGR_URL');
		expect(without.mcpJson).not.toContain('TMGR_URL');
		expect(without.hooksEnvFile).not.toContain('TMGR_URL');

		const withUrl = buildNotifySnippets({
			token: TOKEN,
			apiUrl: 'http://localhost:8080',
		});
		expect(withUrl.claudeCode).toContain("-e 'TMGR_URL=http://localhost:8080'");
		expect(withUrl.codex).toContain('TMGR_URL = "http://localhost:8080"');
		expect(
			JSON.parse(withUrl.mcpJson).mcpServers['tmgr-notify'].env.TMGR_URL,
		).toBe('http://localhost:8080');
		expect(withUrl.hooksEnvFile).toContain("'TMGR_URL=http://localhost:8080'");
	});

	it('escapes quotes in shell and TOML output', () => {
		const snippets = buildNotifySnippets({ token: `a'b "c" \\d` });

		expect(snippets.claudeCode).toContain(
			`-e 'TMGR_NOTIFY_TOKEN=a'\\''b "c" \\d'`,
		);
		expect(snippets.hooksEnvFile).toContain(
			`'TMGR_NOTIFY_TOKEN=a'\\''b "c" \\d'`,
		);
		expect(snippets.codex).toContain(`TMGR_NOTIFY_TOKEN = "a'b \\"c\\" \\\\d"`);
		expect(
			JSON.parse(snippets.mcpJson).mcpServers['tmgr-notify'].env
				.TMGR_NOTIFY_TOKEN,
		).toBe(`a'b "c" \\d`);
	});

	it('produces valid hooks JSON for all three events', () => {
		const hooks = JSON.parse(buildNotifySnippets({}).hooksClaude).hooks;

		expect(hooks.Notification[0].hooks[0]).toEqual({
			type: 'command',
			command: 'tmgr-notify hook notification',
		});
		expect(hooks.UserPromptSubmit[0].hooks[0].command).toBe(
			'tmgr-notify hook prompt',
		);
		expect(hooks.Stop[0].hooks[0].command).toBe('tmgr-notify hook stop');
	});

	it('emits the codex notify line and mcp command', () => {
		const snippets = buildNotifySnippets({});

		expect(snippets.hooksCodex).toBe(
			'notify = ["tmgr-notify", "hook", "codex"]',
		);
		expect(snippets.claudeCode).toContain('-- npx -y @tmgr/notify mcp');
		expect(snippets.codex).toContain('tool_timeout_sec = 660');
	});
});

describe('shellQuote', () => {
	it('wraps spaces and escapes single quotes', () => {
		expect(shellQuote('a b')).toBe("'a b'");
		expect(shellQuote("it's")).toBe("'it'\\''s'");
	});
});

describe('tomlString', () => {
	it('escapes quotes, backslashes and control characters', () => {
		expect(tomlString('a"b')).toBe('"a\\"b"');
		expect(tomlString('a\\b')).toBe('"a\\\\b"');
		expect(tomlString('a\nb\u0001')).toBe('"a\\nb\\u0001"');
	});
});

describe('resolveNotifyApiUrl', () => {
	const origin = 'http://localhost:5173';

	it('returns null for the default production url', () => {
		expect(resolveNotifyApiUrl('https://api.tmgr.dev', origin)).toBeNull();
		expect(resolveNotifyApiUrl('https://api.tmgr.dev/api/', origin)).toBeNull();
		expect(resolveNotifyApiUrl('https://api.tmgr.dev/api', origin)).toBeNull();
		expect(resolveNotifyApiUrl('https://api.tmgr.dev/', origin)).toBeNull();
	});

	it('strips /api and trailing slashes for custom hosts', () => {
		expect(resolveNotifyApiUrl('http://localhost:8080/api/', origin)).toBe(
			'http://localhost:8080',
		);
		expect(resolveNotifyApiUrl('https://example.com/prefix/api/', origin)).toBe(
			'https://example.com/prefix',
		);
		expect(resolveNotifyApiUrl('https://example.com/prefix/', origin)).toBe(
			'https://example.com/prefix',
		);
	});

	it('resolves relative bases against the origin', () => {
		expect(resolveNotifyApiUrl('/api/', origin)).toBe(origin);
		expect(resolveNotifyApiUrl(undefined, origin)).toBe(origin);
	});

	it('returns null on an unparsable base', () => {
		expect(resolveNotifyApiUrl('http://', origin)).toBeNull();
	});
});
