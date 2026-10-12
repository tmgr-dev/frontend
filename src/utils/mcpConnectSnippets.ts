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

export function resolveMcpUrl(_apiBase: string, _origin: string): string {
  throw new Error('not implemented');
}

export function buildMcpSnippets(_input: McpSnippetInput): McpSnippets {
  throw new Error('not implemented');
}
