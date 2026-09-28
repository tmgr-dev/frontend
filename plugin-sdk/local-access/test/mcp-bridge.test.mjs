import { test } from 'node:test';
import assert from 'node:assert';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const binPath = path.join(__dirname, '..', 'bin', 'tmgr-local-mcp.mjs');

function tempSocketPath() {
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'tmgr-lac-'));
	return path.join(dir, 'test.sock');
}

function startServer(socketPath, handler) {
	return new Promise((resolve, reject) => {
		const server = http.createServer(handler);
		server.on('error', reject);
		server.listen(socketPath, () => resolve(server));
	});
}

function stopServer(server, socketPath) {
	return new Promise((resolve) => {
		server.close(() => {
			try {
				fs.unlinkSync(socketPath);
			} catch {
				// already gone
			}
			try {
				fs.rmdirSync(path.dirname(socketPath));
			} catch {
				// not empty or already gone
			}
			resolve();
		});
	});
}

async function runBridge(socketPath, lines) {
	const child = spawn(process.execPath, [binPath, '--socket', socketPath], {
		stdio: ['pipe', 'pipe', 'pipe'],
	});
	let stdout = '';
	let stderr = '';
	child.stdout.on('data', (chunk) => (stdout += chunk));
	child.stderr.on('data', (chunk) => (stderr += chunk));
	for (const line of lines) child.stdin.write(`${JSON.stringify(line)}\n`);
	child.stdin.end();
	const [code] = await once(child, 'exit');
	return { code, stdout, stderr };
}

test('mcp bridge round-trips a request and drops output for a 202 notification', async () => {
	const socketPath = tempSocketPath();
	const received = [];
	const server = await startServer(socketPath, (req, res) => {
		const chunks = [];
		req.on('data', (c) => chunks.push(c));
		req.on('end', () => {
			const body = JSON.parse(Buffer.concat(chunks).toString('utf8'));
			received.push(body);
			if (body.id === undefined) {
				res.writeHead(202);
				res.end();
				return;
			}
			res.writeHead(200, { 'Content-Type': 'application/json' });
			res.end(JSON.stringify({ jsonrpc: '2.0', id: body.id, result: { echoed: body.method } }));
		});
	});

	const { code, stdout } = await runBridge(socketPath, [
		{ jsonrpc: '2.0', id: 1, method: 'ping' },
		{ jsonrpc: '2.0', method: 'notify' },
	]);

	const lines = stdout
		.trim()
		.split('\n')
		.filter(Boolean)
		.map((l) => JSON.parse(l));

	assert.equal(code, 0);
	assert.equal(received.length, 2);
	assert.equal(lines.length, 1);
	assert.deepEqual(lines[0], { jsonrpc: '2.0', id: 1, result: { echoed: 'ping' } });

	await stopServer(server, socketPath);
});

test('mcp bridge emits a JSON-RPC error for a request with an id when the socket is unreachable', async () => {
	const socketPath = path.join(os.tmpdir(), `tmgr-lac-missing-${process.pid}.sock`);

	const { code, stdout, stderr } = await runBridge(socketPath, [
		{ jsonrpc: '2.0', id: 42, method: 'ping' },
	]);

	const lines = stdout
		.trim()
		.split('\n')
		.filter(Boolean)
		.map((l) => JSON.parse(l));

	assert.equal(code, 0);
	assert.equal(lines.length, 1);
	assert.equal(lines[0].id, 42);
	assert.equal(lines[0].error.code, -32000);
	assert.match(lines[0].error.message, /APP_NOT_RUNNING/);
	assert.match(stderr, /APP_NOT_RUNNING/);
});
