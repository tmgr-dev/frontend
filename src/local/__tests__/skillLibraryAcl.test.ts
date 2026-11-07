import { readFileSync } from 'fs';
import { join } from 'path';

const root = join(__dirname, '../../../src-tauri');
const capability = JSON.parse(
	readFileSync(join(root, 'capabilities/skill-library.json'), 'utf8'),
);

const commands = [
	'skill_library_read',
	'skill_library_write',
	'skill_library_export',
	'skill_library_import_pick',
	'skill_library_migrate_local',
];

describe('skill library commands in the desktop ACL', () => {
	it.each(commands)(
		'%s is registered, in the command manifest and allowed',
		(command) => {
			expect(readFileSync(join(root, 'src/lib.rs'), 'utf8')).toContain(
				`skill_library::${command},`,
			);
			expect(readFileSync(join(root, 'build.rs'), 'utf8')).toContain(
				`"${command}",`,
			);
			expect(capability.permissions).toContain(
				`allow-${command.replace(/_/g, '-')}`,
			);
		},
	);

	it('is for the main window only', () => {
		expect(capability.windows).toEqual(['main']);
	});
});
