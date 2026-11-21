<template>
	<SettingsSection :title="$t('agents.usage.machines.chart.title')">
		<template #actions>
			<div class="flex flex-wrap gap-2">
				<div
					class="inline-flex rounded-md border border-border p-0.5"
					role="group"
					:aria-label="$t('agents.usage.machines.chart.stackBy')"
				>
					<button
						v-for="m in stackModes"
						:key="m.key"
						type="button"
						class="rounded px-3 py-1 text-sm"
						:class="
							by === m.key
								? 'bg-muted font-medium text-ink'
								: 'text-ink-subtle hover:text-ink'
						"
						:aria-pressed="by === m.key"
						@click="by = m.key"
					>
						{{ m.label }}
					</button>
				</div>
				<div
					class="inline-flex rounded-md border border-border p-0.5"
					role="group"
					:aria-label="$t('agents.usage.machines.chart.metric')"
				>
					<button
						v-for="m in metricModes"
						:key="m.key"
						type="button"
						class="rounded px-3 py-1 text-sm"
						:class="
							metric === m.key
								? 'bg-muted font-medium text-ink'
								: 'text-ink-subtle hover:text-ink'
						"
						:aria-pressed="metric === m.key"
						@click="metric = m.key"
					>
						{{ m.label }}
					</button>
				</div>
			</div>
		</template>

		<p v-if="chart.empty" class="text-sm text-ink-subtle">
			{{ $t('agents.usage.machines.chart.empty') }}
		</p>
		<template v-else>
			<div class="flex gap-2">
				<div
					class="relative mt-2 h-48 w-14 shrink-0 text-right text-xs text-ink-subtle tabular-nums"
					aria-hidden="true"
				>
					<span
						v-for="tick in ticks"
						:key="tick"
						class="absolute right-0 translate-y-1/2 leading-none"
						:style="{ bottom: pct(tick) + '%' }"
					>
						{{ axis(tick) }}
					</span>
				</div>
				<div class="min-w-0 flex-1">
					<div
						class="relative mt-2 h-48 border-b border-border"
						role="group"
						:aria-label="$t('agents.usage.machines.chart.title')"
						data-testid="machines-chart"
						@mouseleave="active = null"
					>
						<div
							v-for="tick in ticks.slice(1)"
							:key="tick"
							class="absolute inset-x-0 border-t border-border"
							:style="{ bottom: pct(tick) + '%' }"
							aria-hidden="true"
						/>
						<div class="absolute inset-0 flex gap-px">
							<div
								v-for="(d, i) in chart.days"
								:key="d.day"
								role="img"
								tabindex="0"
								class="relative flex h-full min-w-0 flex-1 items-end rounded-sm hover:bg-muted"
								:class="active === i ? 'bg-muted' : ''"
								:aria-label="dayLabel(d)"
								:data-day="d.day"
								@mouseenter="active = i"
								@focus="active = i"
								@blur="active = null"
							>
								<div
									v-if="d.total > 0"
									class="flex w-full flex-col-reverse overflow-hidden rounded-t-sm"
									:style="{ height: pct(d.total) + '%', minHeight: '2px' }"
								>
									<div
										v-for="s in chart.series.filter((x) => d.parts[x.key])"
										:key="s.key"
										:class="swatch(s.key)"
										:style="{ height: (d.parts[s.key] / d.total) * 100 + '%' }"
									/>
								</div>
							</div>
						</div>
						<div
							v-if="activeDay"
							class="pointer-events-none absolute top-0 z-10 w-56 max-w-[calc(100%-1rem)] rounded-md border border-border bg-card p-3 text-xs shadow-md"
							:style="tooltipStyle"
							aria-hidden="true"
						>
							<p class="font-medium text-ink">{{ longDay(activeDay.day) }}</p>
							<p class="text-ink-subtle tabular-nums">
								{{ value(activeDay.total) }}
							</p>
							<ul class="mt-2 flex flex-col gap-1">
								<li
									v-for="s in activeParts"
									:key="s.key"
									class="flex items-center justify-between gap-3"
								>
									<span class="flex min-w-0 items-center gap-1.5">
										<span
											class="h-2 w-2 shrink-0 rounded-sm"
											:class="swatch(s.key)"
										/>
										<span class="truncate text-ink-subtle">{{ s.label }}</span>
									</span>
									<span class="text-ink tabular-nums">{{
										value(activeDay.parts[s.key])
									}}</span>
								</li>
							</ul>
						</div>
					</div>
					<div
						class="relative mt-1 h-4 text-xs text-ink-subtle"
						aria-hidden="true"
					>
						<span
							v-for="i in labelIdx"
							:key="i"
							class="absolute -translate-x-1/2 whitespace-nowrap"
							:style="{ left: ((i + 0.5) / chart.days.length) * 100 + '%' }"
						>
							{{ shortDay(chart.days[i].day) }}
						</span>
					</div>
				</div>
			</div>
			<ul class="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-ink-subtle">
				<li
					v-for="s in chart.series"
					:key="s.key"
					class="flex min-w-0 items-center gap-1.5"
				>
					<span class="h-2 w-2 shrink-0 rounded-sm" :class="swatch(s.key)" />
					<span class="max-w-[16rem] truncate" :title="s.label">{{
						s.label
					}}</span>
				</li>
			</ul>
		</template>
	</SettingsSection>
</template>

<script lang="ts">
	import type { DailyRow, Machine } from '@/actions/tmgr/aiUsageMachines';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { t } from '@/i18n';
	import { axisMoney, count, longDay, money, shortDay } from '@/usage/format';
	import {
		buildDailySeries,
		type Metric,
		type SeriesDay,
		type StackBy,
	} from '@/usage/machines';
	import { computed, defineComponent, ref, type PropType } from 'vue';

	const PALETTE = [
		'bg-sky-500 dark:bg-sky-400',
		'bg-violet-500 dark:bg-violet-400',
		'bg-emerald-500 dark:bg-emerald-400',
		'bg-amber-500 dark:bg-amber-400',
		'bg-rose-500 dark:bg-rose-400',
		'bg-teal-500 dark:bg-teal-400',
	];
	const LABEL_EVERY = 7;

	export default defineComponent({
		name: 'MachineTotalsChart',
		components: { SettingsSection },
		props: {
			rows: { type: Array as PropType<DailyRow[]>, required: true },
			machines: { type: Array as PropType<Machine[]>, required: true },
			range: {
				type: Object as PropType<{ from: string; to: string }>,
				required: true,
			},
		},
		setup(props) {
			const by = ref<StackBy>('machine');
			const metric = ref<Metric>('cost');
			const active = ref<number | null>(null);
			const chart = computed(() =>
				buildDailySeries(props.rows, props.range, {
					by: by.value,
					metric: metric.value,
					machines: props.machines,
				}),
			);
			const axisMax = computed(() => {
				const max = chart.value.max;
				if (max <= 0) return 1;
				const mag = Math.pow(10, Math.floor(Math.log10(max)));
				const step = [1, 2, 5, 10].find((s) => s * mag >= max)! * mag;
				return step;
			});
			const ticks = computed(() => [0, axisMax.value / 2, axisMax.value]);
			const pct = (v: number) => (v / axisMax.value) * 100;
			const value = (v: number) =>
				metric.value === 'cost' ? money(v) : count(v);
			const axis = (v: number) =>
				metric.value === 'cost' ? axisMoney(v) : count(v);
			const swatches = computed(() => {
				const map: Record<string, string> = {};
				chart.value.series.forEach((s, i) => {
					map[s.key] = PALETTE[i % PALETTE.length];
				});
				return map;
			});
			const activeDay = computed(() =>
				active.value === null ? null : chart.value.days[active.value],
			);
			const activeParts = computed(() => {
				const day = activeDay.value;
				if (!day) return [];
				return chart.value.series.filter((s) => day.parts[s.key]);
			});
			const tooltipStyle = computed(() => {
				const i = active.value ?? 0;
				const center = ((i + 0.5) / chart.value.days.length) * 100;
				return i < chart.value.days.length / 2
					? { left: `calc(${center}% + 8px)` }
					: { right: `calc(${100 - center}% + 8px)` };
			});
			const labelIdx = computed(() => {
				const out: number[] = [];
				for (let i = chart.value.days.length - 1; i >= 0; i -= LABEL_EVERY)
					out.unshift(i);
				return out;
			});
			return {
				by,
				metric,
				active,
				chart,
				ticks,
				pct,
				value,
				axis,
				activeDay,
				activeParts,
				tooltipStyle,
				labelIdx,
				swatch: (key: string) => swatches.value[key],
				stackModes: computed(() => [
					{
						key: 'machine' as const,
						label: t('agents.usage.machines.chart.byMachine'),
					},
					{
						key: 'agent' as const,
						label: t('agents.usage.machines.chart.byAgent'),
					},
				]),
				metricModes: computed(() => [
					{
						key: 'cost' as const,
						label: t('agents.usage.machines.chart.cost'),
					},
					{
						key: 'tokens' as const,
						label: t('agents.usage.machines.chart.tokens'),
					},
				]),
				dayLabel: (d: SeriesDay) => `${longDay(d.day)}: ${value(d.total)}`,
				longDay,
				shortDay,
			};
		},
	});
</script>
