<template>
	<div
		class="flex flex-1 flex-col overflow-hidden rounded-card border border-line bg-surface"
	>
		<div class="flex flex-1 flex-col overflow-x-auto overflow-y-hidden">
			<!-- Day headers -->
			<div class="grid border-b border-line" :style="{ gridTemplateColumns: gridCols, minWidth: innerMinWidth }">
				<div />
				<div
					v-for="(d, i) in days"
					:key="i"
					class="px-2 py-2 text-center"
					:class="i === 0 ? '' : 'border-l border-line'"
					:style="isSameDay(d, today) ? { background: 'rgba(232,133,125,.08)' } : undefined"
				>
					<div
						class="text-[10px] font-bold uppercase tracking-wider"
						:class="isSameDay(d, today) ? 'text-brand' : 'text-ink-subtle'"
					>
						{{ dowShort(d) }}
					</div>
					<div
						class="mt-0.5 text-base font-semibold"
						:class="isSameDay(d, today) ? 'text-brand' : 'text-ink'"
					>
						{{ d.getDate() }}
					</div>
				</div>
			</div>

			<!-- All-day row -->
			<div
				v-if="hasAllDay || dragActive"
				class="grid border-b border-line"
				:style="{ gridTemplateColumns: gridCols, minHeight: `${32 + Math.max(maxAllDay,1) * 22}px`, minWidth: innerMinWidth }"
			>
				<div class="pr-2 pt-2 text-right text-[10px] font-bold tracking-wider text-ink-subtle">
					ALL-DAY
				</div>
				<div
					v-for="(d, i) in days"
					:key="i"
					class="flex flex-col gap-0.5 p-1 transition-colors"
					:class="[
						i === 0 ? '' : 'border-l border-line',
						hoverKey === `unsched:${fmtIso(d)}` ? 'bg-brand/15 ring-1 ring-brand/40' : '',
					]"
					:style="isSameDay(d, today) ? { background: 'rgba(232,133,125,.08)' } : undefined"
					data-dr-drop
					data-dr-kind="all-day"
					:data-dr-date="fmtIso(d)"
				>
					<div
						v-for="e in allDayByDay[i]"
						:key="e.task_id + '-' + e.date"
						class="dr-week-chip relative cursor-pointer truncate rounded-sm px-1.5 py-0.5 pr-5 text-2xs text-ink dark:text-white"
						:style="{
							background: hexAlpha(e.routine_category.color, 0.2),
							borderLeft: `2px solid ${e.routine_category.color}`,
							opacity: e.completed ? 0.55 : 1,
							textDecoration: e.completed ? 'line-through' : 'none',
						}"
						:title="e.title"
						@click="$emit('toggle', e)"
					>
						{{ e.title }}
						<button
							type="button"
							class="dr-edit-btn absolute right-0.5 top-1/2 flex h-4 w-4 -translate-y-1/2 items-center justify-center rounded-sm bg-black/30"
							@click.stop="$emit('edit', e)"
						>
							<DRIcon name="pencil" :size="8" stroke="rgba(255,255,255,.85)" />
						</button>
					</div>
				</div>
			</div>

			<!-- Timeline -->
			<div ref="scrollRef" class="flex-1 overflow-y-auto overflow-x-hidden" :style="{ minWidth: innerMinWidth }">
				<div
					class="relative grid"
					:style="{
						gridTemplateColumns: gridCols,
						height: `${24 * HOUR_PX}px`,
					}"
				>
					<div class="relative">
						<div
							v-for="h in 24"
							:key="h"
							class="absolute right-2 text-[10px] text-ink-subtle tabular-nums"
							:style="{ top: `${(h - 1) * HOUR_PX - 6}px` }"
						>
							{{ h === 1 ? '' : String(h - 1).padStart(2, '0') + ':00' }}
						</div>
					</div>

					<div
						v-for="(d, i) in days"
						:key="i"
						class="relative cursor-pointer border-l border-line"
						:style="isSameDay(d, today) ? { background: 'rgba(232,133,125,.08)' } : undefined"
						data-dr-drop
						data-dr-kind="hour-grid"
						:data-dr-date="fmtIso(d)"
						@click="onCellClick($event, d)"
					>
						<div
							v-for="h in 24"
							:key="h"
							class="absolute left-0 right-0"
							:style="{
								top: `${(h - 1) * HOUR_PX}px`,
								height: 0,
								borderTop: h === 1 ? 'none' : '0.5px solid rgba(255,255,255,.06)',
							}"
						/>
						<template v-for="(item, j) in itemsByDay[i]" :key="`d-${i}-${j}`">
							<ClusterBlock
								v-if="item.kind === 'cluster'"
								:events="item.events"
								:start-min="item.startMin"
								:end-min="item.endMin"
								mode="pill"
								@toggle="$emit('toggle', $event)"
								@edit="$emit('edit', $event)"
							/>
							<EventBlock
								v-else
								:entry="item.event.source.entry"
								:start-min="item.event.startMin"
								:end-min="item.event.endMin"
								mode="compact"
								@toggle="$emit('toggle', item.event.source.entry)"
								@edit="$emit('edit', item.event.source.entry)"
							/>
						</template>
						<div
							v-if="isSameDay(d, today) && nowOffset !== null"
							class="absolute left-0 right-0 z-[5]"
							:style="{ top: `${nowOffset}px`, borderTop: '1.5px solid var(--routine-accent, #e8857d)' }"
						>
							<div class="absolute h-2.5 w-2.5 rounded-pill bg-brand" :style="{ left: '-5px', top: '-5px' }" />
						</div>
					</div>
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { computed, onMounted, ref, watch } from 'vue';
	import EventBlock from '../EventBlock.vue';
	import ClusterBlock from '../ClusterBlock.vue';
	import DRIcon from '../DRIcon.vue';
	import { addDays, dowShort, isSameDay, parseTime } from '@/utils/dailyRoutines/dateHelpers';
	import { hexAlpha } from '@/utils/dailyRoutines/categoryMap';
	import { clusterEvents } from '@/utils/dailyRoutines/lanePacking';
	import type { RoutineEntry } from '@/types/dailyRoutine';
	import { useRoutineDrag } from '@/composable/useRoutineDrag';

	const { active, hoverKey } = useRoutineDrag();
	const dragActive = computed(() => !!active.value);

	const HOUR_PX = 56;
	const today = new Date();

	const props = defineProps<{
		entries: RoutineEntry[];
		weekStart: Date;
		isMobile?: boolean;
		isTablet?: boolean;
	}>();

	const emit = defineEmits<{
		(e: 'toggle', entry: RoutineEntry): void;
		(e: 'edit', entry: RoutineEntry): void;
		(e: 'create', payload: { date: string; timeH: number; timeM: number }): void;
	}>();

	function onCellClick(e: MouseEvent, d: Date) {
		const target = e.target as HTMLElement;
		if (!target) return;
		if (target.closest('.dr-week-chip') || target.closest('.dr-edit-btn')) return;
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const y = e.clientY - rect.top;
		const totalMin = Math.max(0, Math.floor((y / HOUR_PX) * 60));
		const snapped = Math.round(totalMin / 30) * 30;
		const h = Math.min(23, Math.floor(snapped / 60));
		const m = snapped % 60;
		emit('create', { date: fmtIso(d), timeH: h, timeM: m });
	}

	const scrollRef = ref<HTMLElement | null>(null);
	const days = computed(() => Array.from({ length: 7 }, (_, i) => addDays(props.weekStart, i)));

	const byDay = computed(() =>
		days.value.map(d => props.entries.filter(e => e.date === fmtIso(d))),
	);

	const allDayByDay = computed(() =>
		byDay.value.map(arr => arr.filter(e => !e.time)),
	);
	const scheduledByDay = computed(() =>
		byDay.value.map(arr => arr.filter(e => !!e.time)),
	);

	const maxAllDay = computed(() =>
		Math.max(1, ...allDayByDay.value.map(a => a.length)),
	);
	const hasAllDay = computed(() => allDayByDay.value.some(a => a.length > 0));

	const itemsByDay = computed(() =>
		scheduledByDay.value.map(dayEntries =>
			clusterEvents(
				dayEntries.map(e => ({
					...e,
					time: parseTime(e.time),
					durationMin: e.duration_min || 30,
					entry: e,
				})),
			),
		),
	);

	const timeColW = computed(() => (props.isMobile ? 40 : 56));
	const dayColMin = computed(() => (props.isMobile ? 90 : props.isTablet ? 110 : 0));
	const gridCols = computed(() => `${timeColW.value}px repeat(7, minmax(${dayColMin.value}px, 1fr))`);
	const innerMinWidth = computed(() => (props.isMobile ? `${timeColW.value + 7 * 90}px` : 'auto'));

	const nowOffset = computed(() => {
		const now = new Date();
		return now.getHours() * HOUR_PX + (now.getMinutes() / 60) * HOUR_PX;
	});

	function fmtIso(d: Date): string {
		const y = d.getFullYear();
		const m = String(d.getMonth() + 1).padStart(2, '0');
		const day = String(d.getDate()).padStart(2, '0');
		return `${y}-${m}-${day}`;
	}

	function scroll() {
		if (scrollRef.value) scrollRef.value.scrollTop = 7 * HOUR_PX - 60;
	}

	onMounted(scroll);
	watch(() => props.weekStart.getTime(), scroll);
</script>
