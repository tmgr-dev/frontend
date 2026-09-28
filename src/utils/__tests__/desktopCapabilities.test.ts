import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const capability = (name: string) =>
	JSON.parse(
		readFileSync(join(__dirname, '../../../src-tauri/capabilities', `${name}.json`), 'utf8'),
	);

describe('main window capabilities', () => {
	it('lets the main window register, unregister and reset its global shortcuts', () => {
		const defaults = capability('default');

		expect(defaults.windows).toContain('main');
		expect(defaults.permissions).toEqual(
			expect.arrayContaining([
				'global-shortcut:allow-register',
				'global-shortcut:allow-unregister',
				'global-shortcut:allow-unregister-all',
			]),
		);
	});
});
