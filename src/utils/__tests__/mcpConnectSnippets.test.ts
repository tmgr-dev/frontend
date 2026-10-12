import { buildMcpSnippets, resolveMcpUrl } from '../mcpConnectSnippets';

const URL_ = 'https://api.tmgr.dev/mcp';
const TOKEN = 'tok_ABC123';

describe('buildMcpSnippets', () => {
	it('builds the Claude Code command', () => {
		expect(buildMcpSnippets({ token: TOKEN, mcpUrl: URL_ }).claudeCode).toBe(
			"claude mcp add --transport http tmgr 'https://api.tmgr.dev/mcp' \\\n  --header 'X-Smart-Device-Token: tok_ABC123'",
		);
	});

	it('builds the Codex TOML with an http_headers table', () => {
		expect(buildMcpSnippets({ token: TOKEN, mcpUrl: URL_ }).codexToml).toBe(
			[
				'[mcp_servers.tmgr]',
				'url = "https://api.tmgr.dev/mcp"',
				'',
				'[mcp_servers.tmgr.http_headers]',
				'"X-Smart-Device-Token" = "tok_ABC123"',
			].join('\n'),
		);
	});

	it('builds mcp.json', () => {
		const json = JSON.parse(
			buildMcpSnippets({ token: TOKEN, mcpUrl: URL_ }).mcpJson,
		);
		expect(json).toEqual({
			mcpServers: {
				tmgr: {
					type: 'http',
					url: URL_,
					headers: { 'X-Smart-Device-Token': TOKEN },
				},
			},
		});
	});

	it('builds a Cursor deeplink whose config round-trips', () => {
		const { cursorDeeplink } = buildMcpSnippets({ token: TOKEN, mcpUrl: URL_ });
		const url = new URL(cursorDeeplink);
		expect(`${url.protocol}//${url.host}${url.pathname}`).toBe(
			'cursor://anysphere.cursor-deeplink/mcp/install',
		);
		expect(url.searchParams.get('name')).toBe('tmgr');
		const decoded = JSON.parse(
			Buffer.from(url.searchParams.get('config') as string, 'base64').toString(
				'utf8',
			),
		);
		expect(decoded).toEqual({
			url: URL_,
			headers: { 'X-Smart-Device-Token': TOKEN },
		});
	});

	it('escapes hostile token characters per format', () => {
		const token = `a"b'c\\d\ne`;
		const s = buildMcpSnippets({ token, mcpUrl: URL_ });
		expect(s.claudeCode).toContain(`'X-Smart-Device-Token: a"b'\\''c\\d\ne'`);
		expect(s.codexToml).toContain('"a\\"b\'c\\\\d\\ne"');
		expect(
			JSON.parse(s.mcpJson).mcpServers.tmgr.headers['X-Smart-Device-Token'],
		).toBe(token);
		const config = new URL(s.cursorDeeplink).searchParams.get(
			'config',
		) as string;
		expect(
			JSON.parse(Buffer.from(config, 'base64').toString('utf8')).headers[
				'X-Smart-Device-Token'
			],
		).toBe(token);
	});
});

describe('resolveMcpUrl', () => {
	it('maps an absolute api base to /mcp', () => {
		expect(resolveMcpUrl('https://api.tmgr.dev/api/', 'https://tmgr.dev')).toBe(
			URL_,
		);
		expect(
			resolveMcpUrl('http://localhost:8080/api/', 'http://localhost:5173'),
		).toBe('http://localhost:8080/mcp');
	});

	it('resolves a relative base against the origin and keeps path prefixes', () => {
		expect(resolveMcpUrl('/api/', 'https://tmgr.dev')).toBe(
			'https://tmgr.dev/mcp',
		);
		expect(resolveMcpUrl(undefined, 'https://tmgr.dev')).toBe(
			'https://tmgr.dev/mcp',
		);
		expect(resolveMcpUrl('https://x.io/tmgr/api', 'https://tmgr.dev')).toBe(
			'https://x.io/tmgr/mcp',
		);
	});

	it('falls back to the default on an invalid base', () => {
		expect(resolveMcpUrl('http://', 'not a url')).toBe(URL_);
	});
});
