const UUID =
	/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

const load = () => {
	let mod: typeof import('../sourceInstance');
	jest.isolateModules(() => {
		mod = require('../sourceInstance');
	});
	return mod!;
};

it('is a canonical UUID v4, stable within the module and fresh per load', () => {
	const first = load();
	expect(first.SOURCE_INSTANCE_ID).toMatch(UUID);
	expect(load().SOURCE_INSTANCE_ID).not.toBe(first.SOURCE_INSTANCE_ID);
});

it('falls back to a canonical UUID v4 without crypto.randomUUID', () => {
	const original = globalThis.crypto;
	Object.defineProperty(globalThis, 'crypto', {
		value: undefined,
		configurable: true,
	});
	try {
		for (let i = 0; i < 50; i++)
			expect(load().SOURCE_INSTANCE_ID).toMatch(UUID);
	} finally {
		Object.defineProperty(globalThis, 'crypto', {
			value: original,
			configurable: true,
		});
	}
});
