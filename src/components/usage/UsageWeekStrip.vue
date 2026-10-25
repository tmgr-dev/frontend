<template>
	<SettingsSection title="Compared with last week">
		<div class="grid grid-cols-2 gap-4 sm:grid-cols-4">
			<div v-for="t in tiles" :key="t.label" :data-testid="t.testid">
				<p class="text-xs font-medium tracking-wide text-ink-subtle uppercase">
					{{ t.label }}
				</p>
				<p class="text-xl font-semibold text-ink tabular-nums">
					{{ t.value }}
				</p>
				<p class="text-xs text-ink-subtle">{{ t.note }}</p>
			</div>
		</div>
	</SettingsSection>
</template>

<script lang="ts">
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import type { UsageDay } from '@/usage/api';
	import { count, money, signedPercent } from '@/usage/format';
	import { averageCostPerDay, weekComparison, type Delta } from '@/usage/stats';
	import { computed, defineComponent, type PropType } from 'vue';

	const note = (d: Delta) =>
		d.pct === null
			? 'no data for the previous 7 days'
			: `${signedPercent(d.pct)} vs previous 7 days`;

	export default defineComponent({
		name: 'UsageWeekStrip',
		components: { SettingsSection },
		props: {
			days: { type: Array as PropType<UsageDay[]>, required: true },
		},
		setup(props) {
			const tiles = computed(() => {
				const today = new Date();
				const week = weekComparison(props.days, today);
				const avg = averageCostPerDay(props.days, today);
				return [
					{
						testid: 'week-cost',
						label: 'Cost, last 7 days',
						value: money(week.cost.current),
						note: note(week.cost),
					},
					{
						testid: 'week-tokens',
						label: 'Tokens, last 7 days',
						value: count(week.tokens.current),
						note: note(week.tokens),
					},
					{
						testid: 'week-events',
						label: 'Events, last 7 days',
						value: count(week.events.current),
						note: note(week.events),
					},
					{
						testid: 'week-average',
						label: 'Average per day',
						value: money(avg.perDay),
						note:
							avg.spanDays >= 30
								? 'over the last 30 days'
								: `over ${avg.spanDays} ${avg.spanDays === 1 ? 'day' : 'days'} since first usage`,
					},
				];
			});
			return { tiles };
		},
	});
</script>
