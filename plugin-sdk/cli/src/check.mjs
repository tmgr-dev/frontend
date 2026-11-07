import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { validate } from '../lib/validate.mjs';

const loadTestHost = async () => {
  try {
    return (await import('tmgr-plugin-testing')).createTestHost;
  } catch (error) {
    if (error.code === 'ERR_MODULE_NOT_FOUND') {
      throw new Error(
        'tmgr-plugin-testing is not installed; run `npm install tmgr-plugin-testing` next to tmgr-plugin',
      );
    }
    throw error;
  }
};

const message = (error) =>
  error instanceof Error ? error.message : String(error);

export const printGroups = (groups, out) => {
  for (const { group, permissions } of groups) {
    out.log(`${group}: ${permissions.join(', ')}`);
  }
};

export const check = async (dir, out = console) => {
  const folder = resolve(dir);
  const result = validate(folder);
  result.warnings.forEach((m) => out.warn(`warning: ${m}`));
  result.errors.forEach((m) => out.error(m));
  if (!result.ok) return { ok: false, manifest: result.manifest };
  const manifest = result.manifest;

  const failures = [];
  let host;
  try {
    const createTestHost = await loadTestHost();
    host = await createTestHost({
      manifest: JSON.parse(readFileSync(join(folder, 'manifest.json'), 'utf8')),
      mainPath: join(folder, manifest.main),
      tasks: [{ title: 'tmgr-plugin check sample task' }],
    });
  } catch (error) {
    out.error(`failed to load ${manifest.main}: ${message(error)}`);
    return { ok: false, manifest };
  }

  try {
    const menuCommands = new Set(
      host.taskMenuItems().map((item) => item.command),
    );
    for (const command of manifest.contributes.commands) {
      try {
        if (menuCommands.has(command.id)) {
          await host.clickTaskMenu(command.id, 1);
        } else {
          await host.runCommand(command.id, null);
        }
        out.log(`command ${command.id}: ok`);
      } catch (error) {
        failures.push(`command ${command.id}`);
        out.error(`command ${command.id}: ${message(error)}`);
      }
    }
    for (const view of manifest.contributes.views.filter((v) => !v.ui)) {
      try {
        await host.renderPage(view.id, null);
        out.log(`view ${view.id}: rendered`);
      } catch (error) {
        failures.push(`view ${view.id}`);
        out.error(`view ${view.id}: ${message(error)}`);
      }
    }
    for (const entry of host.tmgr.log ?? []) {
      out.log(`tmgr.log ${entry.level}: ${entry.message}`);
    }
  } finally {
    host.dispose();
  }

  printGroups(result.groups, out);
  if (failures.length) {
    out.error(`check failed: ${failures.join(', ')}`);
    return { ok: false, manifest };
  }
  out.log(`${manifest.id}: check passed.`);
  return { ok: true, manifest };
};
