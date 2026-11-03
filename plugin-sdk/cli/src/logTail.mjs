import { closeSync, fstatSync, openSync, readSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

export const logFile = (env = process.env, platform = process.platform) => {
  if (env.TMGR_LOG_FILE) return env.TMGR_LOG_FILE;
  if (platform === 'darwin') {
    return join(homedir(), 'Library', 'Logs', 'dev.tmgr.desktop', 'TMGR.log');
  }
  if (platform === 'win32') {
    const base = env.APPDATA ?? join(homedir(), 'AppData', 'Roaming');
    return join(base, 'dev.tmgr.desktop', 'logs', 'TMGR.log');
  }
  return join(
    homedir(),
    '.local',
    'share',
    'dev.tmgr.desktop',
    'logs',
    'TMGR.log',
  );
};

export const createLogTail = ({ file, getId, emit, say }) => {
  let offset = null;
  let inode = null;
  let pending = '';
  let missingReported = false;

  const poll = () => {
    let size;
    try {
      const stat = statSync(file);
      size = stat.size;
      if (inode !== null && stat.ino !== inode) offset = 0;
      inode = stat.ino;
    } catch {
      if (!missingReported) {
        missingReported = true;
        say(
          `app log not found at ${file}; plugin log lines will show once it exists`,
        );
      }
      offset = 0;
      pending = '';
      return;
    }
    if (offset === null) {
      offset = size;
      return;
    }
    if (size < offset) {
      offset = 0;
      pending = '';
    }
    if (size === offset) return;

    const fd = openSync(file, 'r');
    try {
      size = fstatSync(fd).size;
      const buffer = Buffer.alloc(size - offset);
      const read = readSync(fd, buffer, 0, buffer.length, offset);
      offset += read;
      pending += buffer.toString('utf8', 0, read);
    } finally {
      closeSync(fd);
    }
    const lines = pending.split('\n');
    pending = lines.pop() ?? '';
    const id = getId();
    if (!id) return;
    const needle = `[plugin] ${id} `;
    for (const line of lines) {
      if (line.includes(needle)) emit(line.replace(/\r$/, ''));
    }
  };
  return { poll };
};
