<template>
	<div class="flex flex-1 flex-col gap-3 min-h-0">
		<div
			v-if="unscheduled.length || dragActive"
			class="flex shrink-0 flex-col gap-1.5 max-h-[220px] overflow-y-auto rounded-card transition-colors"
			:class="hoverKey === unschedHoverKey ? 'bg-brand/10 ring-1 ring-brand/40' : ''"
			data-dr-drop
			data-dr-kind="unscheduled"
			:data-dr-date="dateIso"
		>
			<div class="flex items-center justify-between gap-2 px-1">
				<div class="text-[10px] font-bold uppercase tracking-wider text-ink-subtle">
					Unscheduled <span>{{ unscheduled.length }}</span>
				</div>
				<button
					v-if="unscheduledDoneCount > 0"
					type="button"
					class="rounded-pill border border-line bg-surface px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider text-ink-subtle hover:border-brand hover:text-brand"
					@click="$emit('archive-done-unscheduled')"
				>
					Archive done ({{ unscheduledDoneCount }})
				</button>
			</div>
			<RoutineRow
				v-for="e in unscheduled"
				:key="`u-${e.task_id}`"
				class="shrink-0"
				:entry="e"
				@toggle="$emit('toggle', e)"
				@edit="$emit('edit', e)"
				@delete="$emit('delete', e)"
				@archive="$emit('archive', e)"
				@select="$emit('select', e)"
				@context="$emit('context', $event)"
			/>
		</div>

		<div
			ref="scrollRef"
			class="flex-1 overflow-auto rounded-card border border-line bg-surface"
		>
			<div
				class="relative cursor-pointer pl-14"
				:style="{ height: `${24 * HOUR_PX}px` }"
				data-dr-drop
				data-dr-kind="hour-grid"
				:data-dr-date="dateIso"
				@click="onCellClick($event)"
			>
				<div
					v-for="h in 24"
					:key="h"
					class="absolute left-0 right-0"
					:style="{
						top: `${(h - 1) * HOUR_PX}px`,
						height: `${HOUR_PX}px`,
						borderTop: h === 1 ? 'none' : '0.5px solid rgba(255,255,255,.06)',
					}"
				>
					<div
						v-if="h > 1"
						class="absolute left-0 w-12 pr-2 text-right text-[10px] text-ink-subtle tabular-nums"
						:style="{ top: '-7px' }"
					>
						{{ String(h - 1).padStart(2, '0') }}:00
					</div>
				</div>

				<div
					v-if="isToday && nowOffset !== null"
					class="absolute z-[5]"
					:style="{
						left: '50px',
						right: '8px',
						top: `${nowOffset}px`,
						borderTop: '1.5px solid var(--routine-accent, #e8857d)',
					}"
				>
					<div
						class="absolute h-2.5 w-2.5 rounded-pill bg-brand"
						:style="{ left: '-5px', top: '-5px' }"
					/>
				</div>

				<template v-for="(item, i) in items" :key="`item-${i}`">
					<ClusterBlock
						v-if="item.kind === 'cluster'"
						:events="item.events"
						:start-min="item.startMin"
						:end-min="item.endMin"
						mode="stack"
						@toggle="$emit('toggle', $event)"
						@edit="$emit('edit', $event)"
					/>
					<EventBlock
						v-else
						:entry="item.event.source.entry"
						:start-min="item.event.startMin"
						:end-min="item.event.endMin"
						mode="full"
						@toggle="$emit('toggle', item.event.source.entry)"
						@edit="$emit('edit', item.event.source.entry)"
						@context="$emit('context', $event)"
					/>
				</template>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { computed, onMounted, ref } from 'vue';
	import RoutineRow from '../RoutineRow.vue';
	import EventBlock from '../EventBlock.vue';
	import ClusterBlock from '../ClusterBlock.vue';
	import { clusterEvents } from '@/utils/dailyRoutines/lanePacking';
	import { isSameDay, parseTime } from '@/utils/dailyRoutines/dateHelpers';
	import type { RoutineEntry } from '@/types/dailyRoutine';
	import { useRoutineDrag } from '@/composable/useRoutineDrag';

	const { active, hoverKey } = useRoutineDrag();
	const dragActive = computed(() => !!active.value);

	const props = defineProps<{
		entries: RoutineEntry[];
		date: Date;
		isMobile?: boolean;
	}>();

	const emit = defineEmits<{
		(e: 'toggle', entry: RoutineEntry): void;
		(e: 'edit', entry: RoutineEntry): void;
		(e: 'delete', entry: RoutineEntry): void;
		(e: 'select', entry: RoutineEntry): void;
		(e: 'archive', entry: RoutineEntry): void;
		(e: 'archive-done-unscheduled'): void;
		(e: 'create', payload: { date: string; timeH: number; timeM: number }): void;
		(e: 'context', payload: { entry: RoutineEntry; x: number; y: number }): void;
	}>();

	function fmtIso(d: Date): string {
		const y = d.getFullYear();
		const m = String(d.getMonth() + 1).padStart(2, '0');
		const day = String(d.getDate()).padStart(2, '0');
		return `${y}-${m}-${day}`;
	}

	function onCellClick(e: MouseEvent) {
		const target = e.target as HTMLElement;
		if (!target) return;
		if (target.closest('.dr-week-chip') || target.closest('.dr-edit-btn')) return;
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const y = e.clientY - rect.top;
		const totalMin = Math.max(0, Math.floor((y / HOUR_PX) * 60));
		const snapped = Math.round(totalMin / 30) * 30;
		const h = Math.min(23, Math.floor(snapped / 60));
		const m = snapped % 60;
		emit('create', { date: fmtIso(props.date), timeH: h, timeM: m });
	}

	const HOUR_PX = 56;
	const scrollRef = ref<HTMLElement | null>(null);

	const dateIso = computed(() => fmtIso(props.date));
	const unschedHoverKey = computed(() => `unsched:${dateIso.value}`);

	const isToday = computed(() => isSameDay(props.date, new Date()));
	const nowOffset = computed(() => {
		if (!isToday.value) return null;
		const now = new Date();
		return now.getHours() * HOUR_PX + (now.getMinutes() / 60) * HOUR_PX;
	});

	const unscheduled = computed(() => props.entries.filter(e => !e.time));
	const unscheduledDoneCount = computed(
		() => unscheduled.value.filter(e => e.completed).length,
	);
	const scheduled = computed(() =>
		props.entries.filter(e => !!e.time)
	);

	const items = computed(() => {
		const evs = scheduled.value.map(e => ({
			...e,
			time: parseTime(e.time),
			durationMin: e.duration_min || 30,
			entry: e,
		}));
		return clusterEvents(evs);
	});

	onMounted(() => {
		if (scrollRef.value) scrollRef.value.scrollTop = 7 * HOUR_PX - 80;
	});
</script>
