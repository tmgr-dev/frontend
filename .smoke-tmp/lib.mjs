import { chromium } from 'playwright';
export const WEB = 'http://localhost:5174';
export const API = 'http://127.0.0.1:8082';
export const SHOTS = process.env.SHOTS;
export async function browser() { return chromium.launch({ executablePath: process.env.PW_EXE }); }
export async function login(b, email, password = 'qa12345!') {
  const ctx = await b.newContext({ viewport: { width: 1400, height: 900 } });
  const p = await ctx.newPage();
  p.on('console', (m) => { if (m.type() === 'error' && process.env.VERBOSE) console.log('  console:', m.text().slice(0, 160)); });
  await p.goto(`${WEB}/login`);
  await p.fill('input[type="email"], input[name="email"]', email);
  await p.fill('input[type="password"]', password);
  await p.keyboard.press('Enter');
  await p.waitForURL((u) => !u.pathname.startsWith('/login'), { timeout: 15000 });
  await p.waitForTimeout(1500);
  return p;
}
export async function api(method, path, { token, body, ws, device } = {}) {
  const headers = { Accept: 'application/json' };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (token) headers.Authorization = `Bearer ${token}`;
  if (device) headers['X-Smart-Device-Token'] = device;
  if (ws) headers['X-Workspace-Id'] = String(ws);
  const res = await fetch(API + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
  const text = await res.text();
  let json = null; try { json = JSON.parse(text); } catch {}
  return { status: res.status, json, text };
}
export async function token(email, password = 'qa12345!') {
  const r = await api('POST', '/api/auth/login', { body: { email, password } });
  return (r.json.data || r.json).token;
}
export const shot = (p, name, full = false) => p.screenshot({ path: `${SHOTS}/${name}.png`, fullPage: full });
