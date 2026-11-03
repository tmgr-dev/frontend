import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { check } from './check.mjs';
import { dev } from './dev.mjs';

const lib = join(dirname(fileURLToPath(import.meta.url)), '..', 'lib');

const USAGE = `Usage: tmgr-plugin <command> [dir] [options]

  validate [dir] [--json] [--strict]  lint the manifest and files, like the app does
  build [dir]                         bundle into dist/tmgr-plugin.json
  check [dir]                         run every command and render every view in the test host
  dev [dir] [--once]                  check, install into the desktop app, watch and tail its log

dir defaults to the current directory.`;

const runLib = (script, dir, extra) =>
  spawnSync(process.execPath, [join(lib, script), ...extra], {
    cwd: dir,
    stdio: 'inherit',
  }).status ?? 1;

export const main = async (argv) => {
  const [command, ...rest] = argv;
  const flags = rest.filter((arg) => arg.startsWith('--'));
  const dir = resolve(
    rest.find((arg) => !arg.startsWith('--')) ?? process.cwd(),
  );

  switch (command) {
    case 'validate':
      return runLib('validate.mjs', dir, [dir, ...flags]);
    case 'build':
      return runLib('build.mjs', dir, []);
    case 'check':
      return (await check(dir)).ok ? 0 : 1;
    case 'dev':
      return dev(dir, { once: flags.includes('--once') });
    case undefined:
    case '--help':
    case '-h':
    case 'help':
      console.log(USAGE);
      return command ? 0 : 1;
    default:
      console.error(`unknown command "${command}"\n\n${USAGE}`);
      return 1;
  }
};
