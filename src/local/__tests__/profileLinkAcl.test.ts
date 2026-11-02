import { readFileSync } from 'fs';
import { join } from 'path';

const root = join(__dirname, '../../../src-tauri');
const read = (file: string) => readFileSync(join(root, file), 'utf8');
const capability = (name: string) =>
	JSON.parse(read(`capabilities/${name}.json`));

const commands: [string, string][] = [
	['local_profile_link', 'local_profile::'],
	['persona_llm_migrate_local', 'persona_llm::'],
	['persona_cache_migrate_local', 'persona_cache::'],
];
const permission = (command: string) => `allow-${command.replace(/_/g, '-')}`;

describe('sign-up migration commands in the desktop ACL', () => {
	it.each(commands)(
		'%s is registered, in the command manifest and allowed by local-profile',
		(command, prefix) => {
			expect(read('src/lib.rs')).toContain(`${prefix}${command},`);
			expect(read('build.rs')).toContain(`"${command}",`);
			expect(capability('local-profile').permissions).toContain(
				permission(command),
			);
		},
	);

	it('are for the main window only', () => {
		expect(capability('local-profile').windows).toEqual(['main']);
		for (const file of ['default', 'plugin-windows']) {
			const permissions: string[] = capability(file).permissions;
			for (const [command] of commands)
				expect(permissions).not.toContain(permission(command));
		}
	});

	it('the clear commands are not exposed', () => {
		for (const command of [
			'persona_llm_clear_for_user',
			'persona_cache_clear_for_user',
		]) {
			expect(read('build.rs')).not.toContain(`"${command}"`);
			expect(JSON.stringify(capability('local-profile'))).not.toContain(
				permission(command),
			);
		}
	});
});
