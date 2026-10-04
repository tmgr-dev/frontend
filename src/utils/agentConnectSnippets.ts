export const TOKEN_PLACEHOLDER = '<your token>';

const DEFAULT_API_URL = 'https://api.tmgr.dev';

export function resolveNotifyApiUrl(
	apiBaseUrl: string | undefined,
	origin: string,
): string | null {
	try {
		const url = new URL(apiBaseUrl || '/api/', origin);
		const basePath = url.pathname.replace(/\/api\/?$/, '').replace(/\/$/, '');
		const resolved = `${url.origin}${basePath}`;

		return resolved === DEFAULT_API_URL ? null : resolved;
	} catch (error) {
		return null;
	}
}

export function shellQuote(value: string): string {
	return `'${value.replace(/'/g, `'\\''`)}'`;
}

export function tomlString(value: string): string {
	const escaped = value.replace(/[\\"\u0000-\u001f\u007f]/g, (char) => {
		switch (char) {
			case '\\':
				return '\\\\';
			case '"':
				return '\\"';
			case '\n':
				return '\\n';
			case '\r':
				return '\\r';
			case '\t':
				return '\\t';
			case '\b':
				return '\\b';
			case '\f':
				return '\\f';
			default:
				return `\\u${char.charCodeAt(0).toString(16).padStart(4, '0')}`;
		}
	});

	return `"${escaped}"`;
}

export interface NotifySnippets {
	claudeCode: string;
	codex: string;
	mcpJson: string;
	hooksEnvFile: string;
	hooksClaude: string;
	hooksCodex: string;
}

export function buildNotifySnippets(options: {
	token?: string | null;
	apiUrl?: string | null;
}): NotifySnippets {
	const token = options.token || TOKEN_PLACEHOLDER;
	const apiUrl = options.apiUrl || null;

	const envPairs: Array<[string, string]> = [['TMGR_NOTIFY_TOKEN', token]];
	if (apiUrl) {
		envPairs.push(['TMGR_URL', apiUrl]);
	}

	const claudeCode = [
		'claude mcp add -s user tmgr-notify',
		...envPairs.map(([key, value]) => `-e ${shellQuote(`${key}=${value}`)}`),
		'-- npx -y @tmgr/notify mcp',
	].join(' \\\n  ');

	const codex = [
		'[mcp_servers.tmgr-notify]',
		'command = "npx"',
		'args = ["-y", "@tmgr/notify", "mcp"]',
		'tool_timeout_sec = 660',
		'',
		'[mcp_servers.tmgr-notify.env]',
		...envPairs.map(([key, value]) => `${key} = ${tomlString(value)}`),
	].join('\n');

	const mcpJson = JSON.stringify(
		{
			mcpServers: {
				'tmgr-notify': {
					command: 'npx',
					args: ['-y', '@tmgr/notify', 'mcp'],
					env: Object.fromEntries(envPairs),
				},
			},
		},
		null,
		2,
	);

	const hooksEnvFile = [
		'npm i -g @tmgr/notify',
		'mkdir -p ~/.config/tmgr-notify',
		`(umask 077; printf '%s\\n' ${envPairs
			.map(([key, value]) => shellQuote(`${key}=${value}`))
			.join(' ')} > ~/.config/tmgr-notify/env)`,
	].join('\n');

	const hook = (name: string) => [
		{ hooks: [{ type: 'command', command: `tmgr-notify hook ${name}` }] },
	];
	const hooksClaude = JSON.stringify(
		{
			hooks: {
				Notification: hook('notification'),
				UserPromptSubmit: hook('prompt'),
				Stop: hook('stop'),
			},
		},
		null,
		2,
	);

	const hooksCodex = 'notify = ["tmgr-notify", "hook", "codex"]';

	return { claudeCode, codex, mcpJson, hooksEnvFile, hooksClaude, hooksCodex };
}
