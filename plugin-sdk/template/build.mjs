// Bundles the plugin into dist/tmgr-plugin.json, the one file a GitHub release needs to carry.
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { validate } from './validate.mjs';

const { ok, errors, warnings } = validate(process.cwd());
warnings.forEach((message) => console.warn(`warning: ${message}`));
if (!ok) {
	errors.forEach((message) => console.error(message));
	process.exit(1);
}

const manifest = JSON.parse(readFileSync('manifest.json', 'utf8'));
const pages = {};
if (existsSync('ui')) {
	for (const entry of readdirSync('ui', { withFileTypes: true })) {
		if (entry.isFile() && entry.name.endsWith('.html')) {
			pages[`ui/${entry.name}`] = readFileSync(`ui/${entry.name}`, 'utf8');
		}
	}
}
const code = readFileSync(manifest.main ?? 'main.js', 'utf8');
mkdirSync('dist', { recursive: true });
writeFileSync('dist/tmgr-plugin.json', JSON.stringify({ manifest, code, pages }));
console.log(`dist/tmgr-plugin.json: ${manifest.id} ${manifest.version}`);
