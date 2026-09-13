import fs from 'fs';
import path from 'path';
import vm from 'vm';

test('service-worker activation retires the legacy private API cache and keeps app assets', async () => {
	const retained = new Set([
		'api-cache',
		'workbox-precache-v2-app',
		'public-images',
	]);
	let activate!: (event: {
		waitUntil: (promise: Promise<unknown>) => void;
	}) => void;
	vm.runInNewContext(
		fs.readFileSync(
			path.join(__dirname, '../../../public/clear-private-api-cache.js'),
			'utf8',
		),
		{
			self: {
				addEventListener: (name: string, listener: typeof activate) => {
					if (name === 'activate') activate = listener;
				},
			},
			caches: { delete: async (name: string) => retained.delete(name) },
		},
	);
	let completion!: Promise<unknown>;
	activate({
		waitUntil: (promise) => {
			completion = promise;
		},
	});
	await completion;
	expect([...retained]).toEqual(['workbox-precache-v2-app', 'public-images']);
});
