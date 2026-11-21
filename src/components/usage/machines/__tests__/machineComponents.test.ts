import { readFileSync } from 'fs';
import { join } from 'path';

const root = join(__dirname, '..', '..', '..', '..');
const read = (path: string) =>
	readFileSync(join(root, path), 'utf8').replace(/\s+/g, ' ');

const card = read('components/usage/machines/MachineLimitCard.vue');
const chart = read('components/usage/machines/MachineTotalsChart.vue');
const page = read('pages/AiUsageMachines.vue');
const aiUsage = read('pages/AiUsage.vue');
const routes = read('router/routes.js');
const sidebar = read('components/general/CustomSidebar.vue');
const en = JSON.parse(
	readFileSync(join(root, 'locales/en/agents.json'), 'utf8'),
);

const hasKey = (key: string) =>
	key
		.split('.')
		.slice(1)
		.reduce<any>((node, part) => node?.[part], en) !== undefined;

describe('MachineLimitCard.vue', () => {
	it('renders one bar per limit row with level colours and no raw red or green', () => {
		expect(card).toContain('v-for="r in rows"');
		expect(card).toContain("critical: 'bg-destructive'");
		expect(card).toMatch(/warn: 'bg-amber-500 dark:bg-amber-400'/);
		expect(card).toContain(':data-level="r.level"');
		expect(card).toContain('role="progressbar"');
	});

	it('shows the reset state instead of a percent and a countdown otherwise', () => {
		expect(card).toContain("r.reset ? $t('agents.usage.machines.limits.reset'");
		expect(card).toContain('v-if="r.countdown"');
		expect(card).toContain('{{ r.asOf }}');
	});

	it('confirms removal and explains how to re-enable', () => {
		expect(card).toContain("emit('remove', props.machine.machine_id)");
		expect(card).toContain("command: 'tmgr usage machine on'");
		expect(card).toContain('data-testid="machine-remove-confirm"');
	});

	it('takes now from the parent and owns no timer', () => {
		expect(card).not.toContain('setInterval');
		expect(card).toContain('now: { type: Date');
	});

	it('has a single root element', () => {
		expect(card).toMatch(/^<template> <article[\s\S]*<\/article> <\/template>/);
	});
});

describe('MachineTotalsChart.vue', () => {
	it('toggles stack by machine or agent and metric cost or tokens', () => {
		expect(chart).toContain("key: 'machine' as const");
		expect(chart).toContain("key: 'agent' as const");
		expect(chart).toContain("key: 'cost' as const");
		expect(chart).toContain("key: 'tokens' as const");
		expect(chart).toContain('buildDailySeries(props.rows, props.range');
	});
});

describe('AiUsageMachines.vue', () => {
	it('runs a single 30 s interval and clears it on unmount', () => {
		expect(page).toContain('const TICK_MS = 30_000');
		expect(page.match(/setInterval\(/g)).toHaveLength(1);
		expect(page).toContain('clearInterval(timer)');
	});

	it('reloads the daily rows when the period or a filter changes', () => {
		expect(page).toContain('watch([period, () => ({ ...filters })]');
		expect(page).toContain('dailyQuery(range.value, filters)');
		expect(page).toContain('fetchDaily(query)');
	});

	it('shows empty, loading and error states with retry', () => {
		expect(page).toContain('data-testid="machines-empty"');
		expect(page).toContain('keypath="agents.usage.machines.empty.body"');
		expect(page).toContain('tmgr usage machine sync');
		expect(page).toContain("$t('agents.usage.machines.loading')");
		expect(
			page.match(/common\.actions\.retry/g)?.length,
		).toBeGreaterThanOrEqual(2);
	});

	it('shows the desktop tab switch only on desktop', () => {
		expect(page).toContain('v-if="isDesktop" #header-actions');
		expect(aiUsage).toContain('<AiUsageTabs current="device" />');
	});
});

describe('wiring', () => {
	it('registers an authenticated, non desktop-only route', () => {
		const block = routes.slice(routes.indexOf("path: '/ai-usage/machines'"));
		const route = block.slice(0, block.indexOf("name: 'AiUsageMachines'") + 30);
		expect(route).toContain("name: 'AiUsageMachines'");
		expect(route).not.toContain('allowedGuests');
		expect(route).not.toContain('desktopOnly');
	});

	it('links web users to the machines page and desktop to the local page', () => {
		expect(sidebar).toContain(
			`:to="isDesktop ? '/ai-usage' : '/ai-usage/machines'"`,
		);
	});
});

describe('i18n keys used by the machines UI', () => {
	it('all exist in the English catalog', () => {
		const sources = [
			card,
			chart,
			page,
			read('components/usage/machines/AiUsageTabs.vue'),
			read('usage/machines.ts'),
		].join(' ');
		const keys = new Set(
			[...sources.matchAll(/agents\.usage\.machines\.[A-Za-z0-9_.]+/g)].map(
				(m) => m[0].replace(/\.$/, ''),
			),
		);
		expect(keys.size).toBeGreaterThan(30);
		for (const key of keys) {
			if (key.endsWith('.')) continue;
			expect([key, hasKey(key)]).toEqual([key, true]);
		}
	});
});
