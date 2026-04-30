<template>
	<div class="flex flex-1 flex-col overflow-hidden rounded-card border border-line bg-surface">
		<div class="grid border-b border-line" :style="{ gridTemplateColumns: 'repeat(7, 1fr)' }">
			<div
				v-for="(d, i) in dowLabels"
				:key="i"
				class="py-2 text-center text-[10px] font-bold tracking-wider text-ink-subtle"
				:class="i === 0 ? '' : 'border-l border-line'"
			>
				{{ d }}
			</div>
		</div>
		<div
			class="grid flex-1"
			:style="{
				gridTemplateColumns: 'repeat(7, 1fr)',
				gridTemplateRows: 'repeat(6, minmax(110px, 1fr))',
			}"
		>
			<div
				v-for="(d, i) in days"
				:key="i"
				class="flex min-h-0 cursor-pointer flex-col gap-0.5 overflow-hidden p-1.5"
				:class="[
					i % 7 === 0 ? '' : 'border-l border-line',
					i < 7 ? '' : 'border-t border-line',
				]"
				:style="cellStyle(d)"
				@click="onCellClick($event, d)"
			>
				<div class="flex items-center justify-end text-xs tabular-nums" :style="dayLabelStyle(d)">
					<span
						v-if="isSameDay(d, today)"
						class="inline-flex h-5 w-5 items-center justify-center rounded-pill bg-brand text-[10px] font-bold text-white"
					>
						{{ d.getDate() }}
					</span>
					<span v-else>{{ d.getDate() }}</span>
				</div>
				<div
					v-for="e in byDay[i].slice(0, maxChips)"
					:key="e.task_id + '-' + e.date"
					class="flex items-center gap-1 truncate rounded-sm px-1.5 py-0.5 text-[10px] cursor-pointer text-ink dark:text-white"
					:style="{
						background: hexAlpha(e.routine_category.color, 0.2),
						borderLeft: `2px solid ${e.routine_category.color}`,
						opacity: e.completed ? 0.55 : 1,
						textDecoration: e.completed ? 'line-through' : 'none',
					}"
					:title="e.title"
					@click.stop="$emit('toggle', e)"
				>
					<span v-if="e.time" class="text-[10px] tabular-nums text-ink-subtle dark:text-white/60">{{ e.time }}</span>
					<span class="truncate">{{ e.title }}</span>
				</div>
				<div v-if="byDay[i].length > maxChips" class="px-1.5 text-[10px] text-ink-subtle">
					+ {{ byDay[i].length - maxChips }} more
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { computed } from 'vue';
	import { addDays, isSameDay, startOfMonth, startOfWeek } from '@/utils/dailyRoutines/dateHelpers';
	import { hexAlpha } from '@/utils/dailyRoutines/categoryMap';
	import type { RoutineEntry } from '@/types/dailyRoutine';

	const today = new Date();

	const props = defineProps<{
		entries: RoutineEntry[];
		monthDate: Date;
		isMobile?: boolean;
	}>();

	const emit = defineEmits<{
		(e: 'toggle', entry: RoutineEntry): void;
		(e: 'select-day', date: Date): void;
		(e: 'create', payload: { date: string; timeH: number; timeM: number }): void;
	}>();

	function fmtIso(d: Date): string {
		const y = d.getFullYear();
		const m = String(d.getMonth() + 1).padStart(2, '0');
		const day = String(d.getDate()).padStart(2, '0');
		return `${y}-${m}-${day}`;
	}

	function onCellClick(e: MouseEvent, d: Date) {
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const ratio = Math.max(0, Math.min(0.999, (e.clientY - rect.top) / rect.height));
		const snapped = Math.round((ratio * 24 * 60) / 30) * 30;
		const h = Math.min(23, Math.floor(snapped / 60));
		const m = snapped % 60;
		emit('create', { date: fmtIso(d), timeH: h, timeM: m });
	}

	const days = computed(() => {
		const first = startOfMonth(props.monthDate);
		const gridStart = startOfWeek(first);
		return Array.from({ length: 42 }, (_, i) => addDays(gridStart, i));
	});

	const byDay = computed(() =>
		days.value.map(d => {
			const iso = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
			return props.entries.filter(e => e.date === iso);
		}),
	);

	const dowLabels = computed(() =>
		props.isMobile ? ['M', 'T', 'W', 'T', 'F', 'S', 'S'] : ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'],
	);

	const maxChips = computed(() => (props.isMobile ? 1 : 3));

	function cellStyle(d: Date) {
		const inMonth = d.getMonth() === props.monthDate.getMonth();
		const isToday = isSameDay(d, today);
		return {
			background: isToday ? 'rgba(232,133,125,.08)' : 'transparent',
			opacity: inMonth ? 1 : 0.35,
		};
	}
	function dayLabelStyle(d: Date) {
		const inMonth = d.getMonth() === props.monthDate.getMonth();
		const isToday = isSameDay(d, today);
		return {
			color: isToday ? '#e8857d' : inMonth ? undefined : 'rgba(255,255,255,.35)',
			fontWeight: isToday ? 700 : 500,
		};
	}
</script>
