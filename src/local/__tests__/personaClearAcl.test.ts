import { readdirSync, readFileSync } from 'fs';
import { join } from 'path';

const root = join(__dirname, '../../../src-tauri');

const capability = (name: string) =>
	JSON.parse(readFileSync(join(root, `capabilities/${name}.json`), 'utf8'));

describe('persona logout cleanup commands in the desktop ACL', () => {
	const commands: [string, string][] = [
		['persona_cache_clear_for_user', 'persona_cache'],
		['persona_llm_clear_for_user', 'persona_llm'],
	];

	it.each(commands)(
		'%s is registered, in the command manifest and allowed for the main window',
		(command, module) => {
			expect(readFileSync(join(root, 'src/lib.rs'), 'utf8')).toContain(
				`${module}::${command},`,
			);
			expect(readFileSync(join(root, 'build.rs'), 'utf8')).toContain(
				`"${command}",`,
			);
			const cap = capability('persona-logout');
			expect(cap.windows).toEqual(['main']);
			expect(cap.permissions).toContain(`allow-${command.replace(/_/g, '-')}`);
		},
	);

	it('no other capability grants the cleanup commands', () => {
		const others = readdirSync(join(root, 'capabilities'))
			.filter(
				(file) => file.endsWith('.json') && file !== 'persona-logout.json',
			)
			.map((file) => readFileSync(join(root, 'capabilities', file), 'utf8'));
		for (const text of others) {
			expect(text).not.toContain('allow-persona-cache-clear-for-user');
			expect(text).not.toContain('allow-persona-llm-clear-for-user');
		}
	});
});
