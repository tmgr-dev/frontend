<template>
	<SettingsSection title="Last 30 days">
		<template v-if="!chart.empty" #actions>
			<div
				class="inline-flex rounded-md border border-border p-0.5"
				role="group"
				aria-label="Group by"
			>
				<button
					v-for="m in modes"
					:key="m.key"
					type="button"
					class="rounded px-3 py-1 text-sm"
					:class="
						mode === m.key
							? 'bg-muted font-medium text-ink'
							: 'text-ink-subtle hover:text-ink'
					"
					:aria-pressed="mode === m.key"
					@click="setMode(m.key)"
				>
					{{ m.label }}
				</button>
			</div>
		</template>

		<p v-if="chart.empty" class="text-sm text-ink-subtle">
			No usage in the last 30 days.
		</p>
		<template v-else>
			<div class="flex gap-2">
				<div
					class="relative mt-2 h-48 w-12 shrink-0 text-right text-xs text-ink-subtle tabular-nums"
					aria-hidden="true"
				>
					<span
						v-for="t in chart.ticks"
						:key="t"
						class="absolute right-0 translate-y-1/2 leading-none"
						:style="{ bottom: pct(t) + '%' }"
					>
						{{ axisMoney(t) }}
					</span>
				</div>
				<div class="min-w-0 flex-1">
					<div
						class="relative mt-2 h-48 border-b border-border"
						role="group"
						aria-label="Cost per day, last 30 days"
						data-testid="usage-chart"
						@mouseleave="active = null"
					>
						<div
							v-for="t in chart.ticks.slice(1)"
							:key="t"
							class="absolute inset-x-0 border-t border-border"
							:style="{ bottom: pct(t) + '%' }"
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
									v-if="d.cost > 0"
									class="flex w-full flex-col-reverse overflow-hidden rounded-t-sm"
									:style="{ height: pct(d.cost) + '%', minHeight: '2px' }"
								>
									<div
										v-for="p in d.parts"
										:key="p.key"
										:class="swatch(p.key)"
										:style="{ height: (p.cost / d.cost) * 100 + '%' }"
									/>
								</div>
							</div>
						</div>
						<div
							v-if="activeDay"
							class="pointer-events-none absolute top-0 z-10 w-56 max-w-[calc(100%-1rem)] rounded-md border border-border bg-card p-3 text-xs shadow-md"
							:style="tooltipStyle"
							aria-hidden="true"
							data-testid="usage-chart-tooltip"
						>
							<p class="font-medium text-ink">{{ longDay(activeDay.day) }}</p>
							<p class="text-ink-subtle tabular-nums">
								{{ money(activeDay.cost) }} ·
								{{ count(activeDay.tokens) }} tokens ·
								{{ count(activeDay.events) }} events
							</p>
							<ul v-if="activeParts.length" class="mt-2 flex flex-col gap-1">
								<li
									v-for="p in activeParts"
									:key="p.key"
									class="flex items-center justify-between gap-3"
								>
									<span class="flex min-w-0 items-center gap-1.5">
										<span
											class="h-2 w-2 shrink-0 rounded-sm"
											:class="swatch(p.key)"
										/>
										<span class="truncate text-ink-subtle">{{
											labels[p.key]
										}}</span>
									</span>
									<span class="text-ink tabular-nums">{{ money(p.cost) }}</span>
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
			<ul
				class="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-ink-subtle"
				aria-label="Legend"
			>
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
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import type { UsageDay } from '@/usage/api';
	import { axisMoney, count, longDay, money, shortDay } from '@/usage/format';
	import {
		buildDailyChart,
		type ChartDay,
		type ChartMode,
	} from '@/usage/stats';
	import { computed, defineComponent, ref, type PropType } from 'vue';

	const PALETTE = [
		'bg-sky-500 dark:bg-sky-400',
		'bg-violet-500 dark:bg-violet-400',
		'bg-emerald-500 dark:bg-emerald-400',
		'bg-amber-500 dark:bg-amber-400',
		'bg-rose-500 dark:bg-rose-400',
	];
	const OTHER = 'bg-slate-400 dark:bg-slate-500';
	const LABEL_EVERY = 7;

	export default defineComponent({
		name: 'UsageDailyChart',
		components: { SettingsSection },
		props: {
			days: { type: Array as PropType<UsageDay[]>, required: true },
		},
		setup(props) {
			const mode = ref<ChartMode>('model');
			const active = ref<number | null>(null);
			const chart = computed(() =>
				buildDailyChart(props.days, mode.value, new Date()),
			);
			const swatches = computed(() => {
				const map: Record<string, string> = {};
				chart.value.series.forEach((s, i) => {
					map[s.key] = s.other ? OTHER : PALETTE[i % PALETTE.length];
				});
				return map;
			});
			const labels = computed(() => {
				const map: Record<string, string> = {};
				for (const s of chart.value.series) map[s.key] = s.label;
				return map;
			});
			const pct = (value: number) => (value / chart.value.axisMax) * 100;
			const sortedParts = (d: ChartDay) =>
				[...d.parts].sort((a, b) => b.cost - a.cost);
			const activeDay = computed(() =>
				active.value === null ? null : chart.value.days[active.value],
			);
			const tooltipStyle = computed(() => {
				const i = active.value ?? 0;
				const center = ((i + 0.5) / chart.value.days.length) * 100;
				return i < chart.value.days.length / 2
					? { left: `calc(${center}% + 8px)` }
					: { right: `calc(${100 - center}% + 8px)` };
			});
			const labelIdx = computed(() => {
				const last = chart.value.days.length - 1;
				const out: number[] = [];
				for (let i = last; i >= 1; i -= LABEL_EVERY) out.unshift(i);
				return out;
			});

			return {
				mode,
				active,
				chart,
				labels,
				labelIdx,
				activeDay,
				tooltipStyle,
				activeParts: computed(() =>
					activeDay.value ? sortedParts(activeDay.value) : [],
				),
				modes: [
					{ key: 'tool' as const, label: 'By tool' },
					{ key: 'model' as const, label: 'By model' },
				],
				setMode: (next: ChartMode) => {
					mode.value = next;
					active.value = null;
				},
				pct,
				swatch: (key: string) => swatches.value[key],
				dayLabel: (d: ChartDay) => {
					const parts = sortedParts(d)
						.map((p) => `${labels.value[p.key]} ${money(p.cost)}`)
						.join(', ');
					return `${longDay(d.day)}: ${money(d.cost)}, ${count(d.tokens)} tokens, ${count(d.events)} events${parts ? `. ${parts}` : ''}`;
				},
				axisMoney,
				count,
				longDay,
				money,
				shortDay,
			};
		},
	});
</script>
