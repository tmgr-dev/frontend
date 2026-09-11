// scripts/precompress.mjs — writes .gz next to every compressible dist file.
import { readdirSync, readFileSync, writeFileSync, statSync } from 'node:fs';
import { join, extname } from 'node:path';
import { gzipSync, constants } from 'node:zlib';

const ROOT = new URL('../dist/', import.meta.url).pathname;
const EXT = new Set(['.js', '.css', '.svg', '.json', '.html', '.webmanifest', '.txt', '.xml']);
const MIN = 1024;

function walk(dir) {
	for (const name of readdirSync(dir)) {
		const p = join(dir, name);
		if (statSync(p).isDirectory()) walk(p);
		else if (EXT.has(extname(p)) && statSync(p).size >= MIN) {
			writeFileSync(p + '.gz', gzipSync(readFileSync(p), { level: constants.Z_BEST_COMPRESSION }));
		}
	}
}
walk(ROOT);
