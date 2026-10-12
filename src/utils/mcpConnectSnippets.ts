import { shellQuote, tomlString } from './agentConnectSnippets';

export interface McpSnippetInput {
	token: string;
	mcpUrl: string;
}

export interface McpSnippets {
	claudeCode: string;
	codexToml: string;
	cursorDeeplink: string;
	mcpJson: string;
}

export const MCP_TOKEN_HEADER = 'X-Smart-Device-Token';
export const MCP_SERVER_NAME = 'tmgr';
const DEFAULT_MCP_URL = 'https://api.tmgr.dev/mcp';

export function resolveMcpUrl(
	apiBase: string | undefined,
	origin: string,
): string {
	try {
		const url = new URL(apiBase || '/api/', origin);
		const basePath = url.pathname.replace(/\/api\/?$/, '').replace(/\/$/, '');
		url.pathname = `${basePath}/mcp`;
		url.search = '';
		url.hash = '';
		return url.toString();
	} catch (error) {
		return DEFAULT_MCP_URL;
	}
}

const toBase64 = (text: string): string => {
	const bytes = new TextEncoder().encode(text);
	let binary = '';
	bytes.forEach((byte) => {
		binary += String.fromCharCode(byte);
	});
	return btoa(binary);
};

export function buildMcpSnippets({
	token,
	mcpUrl,
}: McpSnippetInput): McpSnippets {
	const claudeCode = [
		`claude mcp add --transport http ${MCP_SERVER_NAME} ${shellQuote(mcpUrl)}`,
		`--header ${shellQuote(`${MCP_TOKEN_HEADER}: ${token}`)}`,
	].join(' \\\n  ');

	const codexToml = [
		`[mcp_servers.${MCP_SERVER_NAME}]`,
		`url = ${tomlString(mcpUrl)}`,
		'',
		`[mcp_servers.${MCP_SERVER_NAME}.http_headers]`,
		`${tomlString(MCP_TOKEN_HEADER)} = ${tomlString(token)}`,
	].join('\n');

	const server = { url: mcpUrl, headers: { [MCP_TOKEN_HEADER]: token } };
	const cursorDeeplink = `cursor://anysphere.cursor-deeplink/mcp/install?name=${MCP_SERVER_NAME}&config=${encodeURIComponent(
		toBase64(JSON.stringify(server)),
	)}`;

	const mcpJson = JSON.stringify(
		{
			mcpServers: {
				[MCP_SERVER_NAME]: {
					type: 'http',
					url: mcpUrl,
					headers: { [MCP_TOKEN_HEADER]: token },
				},
			},
		},
		null,
		2,
	);

	return { claudeCode, codexToml, cursorDeeplink, mcpJson };
}
