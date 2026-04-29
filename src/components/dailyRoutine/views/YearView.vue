<template>
	<div class="flex flex-1 flex-col overflow-auto rounded-card border border-line bg-surface p-1">
		<div class="grid gap-4 p-4" :style="{ gridTemplateColumns: `repeat(${cols}, 1fr)` }">
			<div v-for="m in months" :key="m.getMonth()">
				<div class="mb-2 text-xs font-semibold text-ink">{{ monthShort(m) }} {{ m.getFullYear() }}</div>
				<div class="grid gap-0.5" :style="{ gridTemplateColumns: 'repeat(7, 1fr)' }">
					<div v-for="(d, i) in dowLabels" :key="i" class="text-center text-[9px] text-ink-subtle">{{ d }}</div>
					<template v-for="(day, j) in monthDays(m)" :key="j">
						<div v-if="day === null" />
						<div
							v-else
							class="aspect-square cursor-pointer rounded-sm"
							:style="cellStyle(day)"
							:title="`${day.toDateString()} — ${stat(day).completed}/${stat(day).fires}`"
							@click="$emit('select-day', day)"
						/>
					</template>
				</div>
			</div>
		</div>
		<div class="flex items-center justify-end gap-2 px-5 pb-4 text-xs text-ink-subtle">
			<span>Less</span>
			<span
				v-for="v in [0, 0.25, 0.5, 0.75, 1]"
				:key="v"
				class="h-3 w-3 rounded-sm"
				:style="{ background: heatColor(v) }"
			/>
			<span>More</span>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { computed } from 'vue';
	import { addDays, isSameDay, monthShort, startOfMonth, startOfWeek } from '@/utils/dailyRoutines/dateHelpers';
	import type { YearStats } from '@/types/dailyRoutine';

	const today = new Date();

	const props = defineProps<{
		year: number;
		stats: YearStats;
		width?: number;
	}>();

	defineEmits<{
		(e: 'select-day', date: Date): void;
	}>();

	const cols = computed(() => {
		const w = props.width ?? 1280;
		if (w < 480) return 1;
		if (w < 720) return 2;
		if (w < 1100) return 3;
		return 4;
	});

	const months = computed(() => Array.from({ length: 12 }, (_, i) => new Date(props.year, i, 1)));

	const dowLabels = ['M', 'T', 'W', 'T', 'F', 'S', 'S'];

	function monthDays(m: Date): (Date | null)[] {
		const first = startOfMonth(m);
		const gridStart = startOfWeek(first);
		const out: (Date | null)[] = [];
		for (let i = 0; i < 42; i++) {
			const d = addDays(gridStart, i);
			if (d.getMonth() !== m.getMonth()) {
				out.push(null);
			} else {
				out.push(d);
			}
			if (i >= 35 && d.getMonth() !== m.getMonth()) break;
		}
		return out;
	}

	function stat(d: Date): { fires: number; completed: number } {
		const iso = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
		return props.stats[iso] ?? { fires: 0, completed: 0 };
	}

	function cellStyle(d: Date) {
		const s = stat(d);
		const ratio = s.fires === 0 ? 0 : s.completed / s.fires;
		const intensity = s.fires === 0 ? 0 : Math.max(0.15, ratio);
		return {
			background: heatColor(intensity),
			border: isSameDay(d, today) ? '1px solid #e8857d' : '1px solid transparent',
		};
	}

	function heatColor(v: number): string {
		if (v <= 0) return 'rgba(127,127,127,.12)';
		const stops: [number, string][] = [
			[0.2, 'rgba(232,133,125,0.18)'],
			[0.4, 'rgba(232,133,125,0.34)'],
			[0.6, 'rgba(232,133,125,0.55)'],
			[0.85, 'rgba(232,133,125,0.78)'],
			[1.0, 'rgba(232,133,125,1.00)'],
		];
		for (const [t, c] of stops) if (v <= t) return c;
		return stops[stops.length - 1][1];
	}
</script>
