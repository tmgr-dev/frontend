import { deepStrictEqual, strictEqual } from 'node:assert';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { createTestHost } from 'tmgr-plugin-testing';

const manifest = JSON.parse(readFileSync(new URL('./manifest.json', import.meta.url), 'utf8'));

test('counts tasks in an active status and shows them on a page', async () => {
	const host = await createTestHost({
		manifest,
		mainPath: new URL('./main.js', import.meta.url),
		statuses: [
			{ name: 'In progress', type: 'active' },
			{ name: 'Done', type: 'completed' },
		],
		tasks: [
			{ title: 'Write docs', status_id: 1 },
			{ title: 'Ship it', status_id: 2 },
		],
	});

	await host.runCommand('yourname.hello.refresh', null);
	strictEqual(host.tmgr.statusBar.count.text, '1 in progress');

	const page = await host.renderPage('summary', null);
	deepStrictEqual(
		page.children[1].items.map((item) => item.text),
		['Write docs'],
	);

	host.dispose();
});
