import { readFileSync } from 'fs';
import { join } from 'path';

const root = join(__dirname, '../../../src-tauri');

const capability = (name: string) =>
	JSON.parse(readFileSync(join(root, `capabilities/${name}.json`), 'utf8'));

describe('usage consent commands in the desktop ACL', () => {
	const commands: [string, string][] = [
		['usage_consent', 'usage'],
		['usage_consent_set', 'usage-consent'],
		['usage_forget', 'usage-consent'],
	];

	it.each(commands)(
		'%s is registered, in the command manifest and allowed by %s',
		(command, file) => {
			expect(readFileSync(join(root, 'src/lib.rs'), 'utf8')).toContain(
				`usage::${command},`,
			);
			expect(readFileSync(join(root, 'build.rs'), 'utf8')).toContain(
				`"${command}",`,
			);
			const cap = capability(file);
			expect(cap.windows).toContain('main');
			expect(cap.permissions).toContain(`allow-${command.replace(/_/g, '-')}`);
		},
	);

	it('changing the choice and forgetting are for the main window only', () => {
		expect(capability('usage-consent').windows).toEqual(['main']);
		const shared = capability('usage').permissions;
		expect(shared).not.toContain('allow-usage-consent-set');
		expect(shared).not.toContain('allow-usage-forget');
	});
});
