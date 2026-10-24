<template>
	<section
		data-activity-chart
		:aria-labelledby="titleId"
		class="min-w-0 rounded-card border border-line bg-surface p-4 shadow-tmgr-xs"
	>
		<h3 :id="titleId" class="text-base font-semibold text-ink">
			Activity over time
		</h3>
		<p v-if="subtitle" class="mt-0.5 text-xs text-ink-muted">
			{{ subtitle }}
		</p>

		<div class="mt-4">
			<Skeleton v-if="loading" class="h-40 w-full sm:h-[200px]" />
			<div
				v-else-if="error"
				class="flex h-40 flex-col items-center justify-center gap-2 text-center sm:h-[200px]"
			>
				<p class="text-sm text-ink-muted">Couldn't load chart data</p>
				<Button
					type="button"
					variant="outline"
					size="sm"
					@click="$emit('retry')"
				>
					Retry
				</Button>
			</div>
			<div
				v-else-if="isEmpty"
				data-chart-empty
				class="flex h-40 items-center justify-center text-center text-sm text-ink-muted sm:h-[200px]"
			>
				No activity in this period
			</div>
			<template v-else>
				<div role="img" :aria-label="summary">
					<svg
						aria-hidden="true"
						focusable="false"
						class="block h-40 w-full sm:h-[200px]"
						viewBox="0 0 100 100"
						preserveAspectRatio="none"
					>
						<g class="text-line">
							<line
								v-for="y in gridYs"
								:key="y"
								x1="0"
								x2="100"
								:y1="y"
								:y2="y"
								stroke="currentColor"
								stroke-width="1"
								vector-effect="non-scaling-stroke"
							/>
						</g>
						<g v-for="line in lines" :key="line.key" :class="line.toneClass">
							<polyline
								:points="line.points"
								fill="none"
								stroke="currentColor"
								stroke-width="2"
								stroke-linecap="round"
								stroke-linejoin="round"
								vector-effect="non-scaling-stroke"
							/>
						</g>
					</svg>
					<div
						class="mt-2 flex justify-between gap-2 text-2xs text-ink-subtle tabular-nums"
						aria-hidden="true"
					>
						<span v-for="tick in xTicks" :key="tick.index" class="truncate">
							{{ tick.label }}
						</span>
					</div>
				</div>

				<div
					class="mt-3 flex flex-wrap items-center justify-between gap-x-4 gap-y-1"
				>
					<ul class="flex min-w-0 flex-wrap gap-x-4 gap-y-1">
						<li
							v-for="item in series"
							:key="item.key"
							class="flex min-w-0 items-center gap-1.5 text-xs text-ink-muted"
						>
							<span
								:class="[
									'h-2 w-2 shrink-0 rounded-full bg-current',
									toneLine[item.tone],
								]"
								aria-hidden="true"
							/>
							<span class="truncate">{{ item.label }}</span>
						</li>
					</ul>
					<p class="text-2xs text-ink-muted">
						Each line is scaled to its own peak
					</p>
				</div>

				<table class="sr-only">
					<caption>
						Activity over time
					</caption>
					<thead>
						<tr>
							<th scope="col">Date</th>
							<th v-for="item in series" :key="item.key" scope="col">
								{{ item.label }}
							</th>
						</tr>
					</thead>
					<tbody>
						<tr v-for="(date, i) in dates" :key="date">
							<th scope="row">{{ formatDay(date) }}</th>
							<td v-for="item in series" :key="item.key">
								{{ item.format(item.values[i] ?? 0) }}
							</td>
						</tr>
					</tbody>
				</table>
			</template>
		</div>
	</section>
</template>

<script lang="ts">
	import Button from '@/components/ui/button/Button.vue';
	import Skeleton from '@/components/ui/skeleton/Skeleton.vue';
	import type { DashboardTone } from '@/types/dashboard';
	import { computed, defineComponent, type PropType } from 'vue';

	export interface ActivityChartSeries {
		key: string;
		label: string;
		tone: DashboardTone;
		values: number[];
		format: (v: number) => string;
		showTotal?: boolean;
	}

	const TONE_LINE: Record<DashboardTone, string> = {
		brand: 'text-brand',
		done: 'text-status-done',
		testing: 'text-status-testing',
		progress: 'text-status-progress',
	};

	const PLOT_TOP = 4;
	const PLOT_BOTTOM = 96;

	let instanceCounter = 0;

	function formatDay(date: string): string {
		return new Date(`${date}T00:00:00Z`).toLocaleDateString(undefined, {
			month: 'short',
			day: 'numeric',
			timeZone: 'UTC',
		});
	}

	export default defineComponent({
		name: 'ActivityChart',
		components: { Button, Skeleton },
		props: {
			dates: { type: Array as PropType<string[]>, required: true },
			series: {
				type: Array as PropType<ActivityChartSeries[]>,
				required: true,
			},
			subtitle: { type: String, default: '' },
			loading: { type: Boolean, default: false },
			error: { type: Boolean, default: false },
		},
		emits: ['retry'],
		setup(props) {
			instanceCounter += 1;
			const titleId = `activity-chart-title-${instanceCounter}`;

			const valuesOf = (s: ActivityChartSeries) =>
				props.dates.map((_, i) => s.values[i] ?? 0);

			const isEmpty = computed(
				() =>
					props.dates.length === 0 ||
					props.series.every((s) => valuesOf(s).every((v) => !v)),
			);

			const gridYs = Array.from(
				{ length: 4 },
				(_, i) => PLOT_TOP + ((PLOT_BOTTOM - PLOT_TOP) * i) / 3,
			);

			const lines = computed(() => {
				const last = props.dates.length - 1;
				return props.series.map((s) => {
					const values = valuesOf(s);
					const max = Math.max(0, ...values);
					const points = values
						.map((v, i) => {
							const x = last > 0 ? (i / last) * 100 : 50;
							const y =
								max > 0
									? PLOT_BOTTOM -
										(Math.max(0, v) / max) * (PLOT_BOTTOM - PLOT_TOP)
									: PLOT_BOTTOM;
							return `${x},${y}`;
						})
						.join(' ');
					return { key: s.key, toneClass: TONE_LINE[s.tone], points };
				});
			});

			const xTicks = computed(() => {
				const n = props.dates.length;
				const indices = [...new Set([0, Math.floor((n - 1) / 2), n - 1])];
				return indices
					.filter((i) => i >= 0)
					.map((index) => ({ index, label: formatDay(props.dates[index]) }));
			});

			const summary = computed(() => {
				const parts = props.series.map((s) => {
					const values = valuesOf(s);
					const total = values.reduce((sum, v) => sum + v, 0);
					const max = Math.max(0, ...values);
					const peak =
						max > 0
							? `, peak ${s.format(max)} on ${formatDay(
									props.dates[values.indexOf(max)],
								)}`
							: '';
					if (s.showTotal === false)
						return `${s.label}: ${peak ? peak.slice(2) : 'none'}.`;
					return `${s.label}: total ${s.format(total)}${peak}.`;
				});
				return [
					`Activity over time, last ${props.dates.length} days.`,
					...parts,
				].join(' ');
			});

			return {
				titleId,
				isEmpty,
				gridYs,
				lines,
				xTicks,
				summary,
				toneLine: TONE_LINE,
				formatDay,
			};
		},
	});
</script>
