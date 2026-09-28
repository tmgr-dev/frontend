import { test } from 'node:test';
import assert from 'node:assert';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createLocalAccessClient, LocalAccessError } from '../index.mjs';

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

test('request() returns the {data} envelope untouched', async () => {
	const socketPath = tempSocketPath();
	const server = await startServer(socketPath, (req, res) => {
		res.writeHead(200, { 'Content-Type': 'application/json' });
		res.end(JSON.stringify({ data: { id: 1, title: 'hello' } }));
	});
	const client = createLocalAccessClient({ socketPath, token: 'tmgrl_test' });

	const result = await client.get('/api/tasks/1');
	assert.deepEqual(result, { data: { id: 1, title: 'hello' } });

	await stopServer(server, socketPath);
});

test('a non-2xx response is rejected with a LocalAccessError carrying the server code', async () => {
	const socketPath = tempSocketPath();
	const server = await startServer(socketPath, (req, res) => {
		res.writeHead(401, { 'Content-Type': 'application/json' });
		res.end(JSON.stringify({ message: 'Unknown or invalid token', code: 'TOKEN_INVALID' }));
	});
	const client = createLocalAccessClient({ socketPath, token: 'tmgrl_bad' });

	await assert.rejects(
		() => client.whoami(),
		(err) => {
			assert.ok(err instanceof LocalAccessError);
			assert.equal(err.status, 401);
			assert.equal(err.code, 'TOKEN_INVALID');
			assert.equal(err.message, 'Unknown or invalid token');
			return true;
		},
	);

	await stopServer(server, socketPath);
});

test('retries a 503 APP_NOT_READY honoring Retry-After, then succeeds', async () => {
	const socketPath = tempSocketPath();
	let attempts = 0;
	const server = await startServer(socketPath, (req, res) => {
		attempts += 1;
		if (attempts === 1) {
			res.writeHead(503, { 'Content-Type': 'application/json', 'Retry-After': '0' });
			res.end(JSON.stringify({ message: 'App is not ready', code: 'APP_NOT_READY' }));
			return;
		}
		res.writeHead(200, { 'Content-Type': 'application/json' });
		res.end(JSON.stringify({ data: { ok: true } }));
	});
	const client = createLocalAccessClient({ socketPath, token: 'tmgrl_test' });

	const result = await client.request('GET', '/api/tasks/1');
	assert.deepEqual(result, { data: { ok: true } });
	assert.equal(attempts, 2);

	await stopServer(server, socketPath);
});

test('a socket that does not exist is reported as APP_NOT_RUNNING', async () => {
	const socketPath = path.join(os.tmpdir(), `tmgr-lac-missing-${process.pid}.sock`);
	const client = createLocalAccessClient({ socketPath, token: 'tmgrl_test' });

	await assert.rejects(
		() => client.whoami(),
		(err) => {
			assert.ok(err instanceof LocalAccessError);
			assert.equal(err.status, 0);
			assert.equal(err.code, 'APP_NOT_RUNNING');
			return true;
		},
	);
});
