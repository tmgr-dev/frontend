import { browser, WEB, api, token, shot } from './lib.mjs';
const results = [];
const check = (name, ok, extra = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${extra ? ' — ' + extra : ''}`); };
const t1 = await token('qa1@tmgr.dev');
const b = await browser();
const ctx = await b.newContext({ viewport: { width: 1400, height: 900 } });
const p = await ctx.newPage();
const disabled = [];
p.on('response', async (r) => {
  if (r.status() !== 403) return;
  try { const j = await r.json(); if (j?.error === 'feature_disabled') disabled.push(`${r.request().method()} ${new URL(r.url()).pathname}`); } catch {}
});
await p.goto(`${WEB}/login`);
await p.fill('input[type="email"], input[name="email"]', 'qa1@tmgr.dev');
await p.fill('input[type="password"]', 'qa12345!');
await p.keyboard.press('Enter');
await p.waitForURL((u) => !u.pathname.startsWith('/login'), { timeout: 15000 });
const toastText = async () => (await p.locator('body').innerText()).match(/is off in workspace|turned off in this workspace|Module is off/i)?.[0] || '';

async function sweep(code, label) {
  const tasks = (await api('GET', `/api/tasks?workspace_id=${code.id}&per_page=5`, { token: t1, ws: code.id })).json;
  const first = (Array.isArray(tasks) ? tasks : tasks.data)[0];
  const routes = ['list', 'board', 'dashboard', 'routines', 'files', 'archive', 'projects-categories', first ? `tasks/${first.id}` : null].filter(Boolean)
    .map((r) => `/${code.code}/${r}`).concat(['/settings/modules', '/settings/workspaces', '/profile']);
  for (const r of routes) {
    disabled.length = 0;
    await p.goto(WEB + r); await p.waitForTimeout(800);
    await p.reload(); await p.waitForTimeout(3500);
    const toast = await toastText();
    check(`${label} reload ${r}: no feature_disabled calls, no toast`, disabled.length === 0 && !toast, [disabled.join(', '), toast].filter(Boolean).join(' | '));
  }
  return first;
}

const first12 = await sweep({ id: 12, code: 'qaownersworkspace' }, '[all off + core hidden]');
await sweep({ id: 14, code: 'existing' }, '[routines off]');

await p.goto(`${WEB}/qaownersworkspace/tasks/${first12.id}`); await p.waitForTimeout(3500);
const body = await p.locator('body').innerText();
check('hidden Comments: no composer/Activity in the task', !/Write a comment|No comments yet/.test(body));
check('hidden Timer: no "0 minute" grammar anywhere', !/\b0 minute\b/.test(body));
await shot(p, 'R1-task-core-hidden');
await p.goto(`${WEB}/qaownersworkspace/list`); await p.waitForTimeout(3000);
const list = await p.locator('body').innerText();
const side = (await p.locator('[data-sidebar=sidebar], aside').first().innerText()).split('\n').map((s) => s.trim());
check('hidden Board/Projects: not in the sidebar', !side.includes('Board') && !side.includes('Categories'));
check('hidden Timer: no time total in the List header', !/\b\d+ minutes?\b/.test(list.split('\n').slice(0, 20).join(' ')));
await shot(p, 'R2-list-core-hidden');
await b.close();
const failed = results.filter((x) => !x).length;
console.log(`\n${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
