#!/usr/bin/env node
import readline from 'node:readline';
import { createLocalAccessClient, resolveSocketPath, LocalAccessError } from '../index.mjs';

function parseArgs(argv) {
	const args = { tokenId: undefined, socket: undefined };
	for (let i = 0; i < argv.length; i++) {
		if (argv[i] === '--token-id') args.tokenId = argv[++i];
		else if (argv[i] === '--socket') args.socket = argv[++i];
	}
	return args;
}

async function main() {
	const { tokenId, socket } = parseArgs(process.argv.slice(2));
	const socketPath = socket || resolveSocketPath();
	const client = createLocalAccessClient({ socketPath, tokenId, retry: false });

	const rl = readline.createInterface({ input: process.stdin, terminal: false });
	for await (const line of rl) {
		const trimmed = line.trim();
		if (!trimmed) continue;

		let message;
		try {
			message = JSON.parse(trimmed);
		} catch (err) {
			process.stderr.write(`tmgr-local-mcp: invalid JSON-RPC line: ${err.message}\n`);
			continue;
		}

		try {
			const response = await client.mcpCall(message);
			if (response !== undefined) process.stdout.write(`${JSON.stringify(response)}\n`);
		} catch (err) {
			const code = err instanceof LocalAccessError ? err.code : 'TRANSPORT_ERROR';
			const detail = err instanceof Error ? err.message : String(err);
			process.stderr.write(`tmgr-local-mcp: request failed (${code}): ${detail}\n`);
			if (message && message.id !== undefined && message.id !== null) {
				const errorResponse = {
					jsonrpc: '2.0',
					id: message.id,
					error: { code: -32000, message: `${code}: ${detail}` },
				};
				process.stdout.write(`${JSON.stringify(errorResponse)}\n`);
			}
		}
	}
}

main().catch((err) => {
	process.stderr.write(`tmgr-local-mcp: fatal: ${err instanceof Error ? err.message : String(err)}\n`);
	process.exitCode = 1;
});
