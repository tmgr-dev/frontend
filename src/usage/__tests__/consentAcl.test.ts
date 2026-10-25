import { readFileSync } from 'fs';
import { join } from 'path';

const root = join(__dirname, '../../../src-tauri');

describe('usage consent commands in the desktop ACL', () => {
	const commands = ['usage_consent', 'usage_consent_set', 'usage_forget'];

	it.each(commands)(
		'%s is registered, in the command manifest and allowed for the usage windows',
		(command) => {
			expect(readFileSync(join(root, 'src/lib.rs'), 'utf8')).toContain(
				`usage::${command},`,
			);
			expect(readFileSync(join(root, 'build.rs'), 'utf8')).toContain(
				`"${command}",`,
			);
			const capability = JSON.parse(
				readFileSync(join(root, 'capabilities/usage.json'), 'utf8'),
			);
			expect(capability.permissions).toContain(
				`allow-${command.replace(/_/g, '-')}`,
			);
		},
	);
});
