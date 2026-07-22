<template>
	<div
		class="dr-week-chip group absolute cursor-pointer overflow-hidden rounded-md transition-opacity"
		:class="[
			entry.completed ? 'opacity-50' : 'opacity-100',
			isBeingDragged ? 'opacity-30' : '',
		]"
		:style="blockStyle"
		:title="`${entry.title} · ${entry.time} – ${endTimeStr}`"
		@click="onClick"
		@pointerdown="onPointerDown($event, entry)"
		@contextmenu.prevent="$emit('context', { entry, x: $event.clientX, y: $event.clientY })"
	>
		<div
			v-if="!isShort"
			class="truncate pr-5 text-xs font-medium text-ink dark:text-white"
			:class="entry.completed ? 'line-through' : ''"
		>
			{{ entry.title }}
		</div>
		<div
			v-if="!isShort"
			class="mt-0.5 truncate pr-5 text-[10px] text-ink-subtle dark:text-white/60"
		>
			{{ entry.time }}<span v-if="endTimeStr"> – {{ endTimeStr }}</span>
		</div>
		<div
			v-else
			class="flex items-center gap-1.5 truncate pr-5 text-[11px] font-medium text-ink dark:text-white"
			:class="entry.completed ? 'line-through' : ''"
		>
			<span class="shrink-0 tabular-nums text-ink-subtle dark:text-white/70">{{ entry.time }}</span>
			<span class="truncate">{{ entry.title }}</span>
		</div>
		<button
			v-if="!isShort"
			type="button"
			class="dr-edit-btn absolute right-1 top-1 flex h-[18px] w-[18px] items-center justify-center rounded-md bg-black/35"
			title="Edit"
			@click.stop="$emit('edit', entry)"
		>
			<DRIcon name="pencil" :size="9" stroke="rgba(255,255,255,.85)" />
		</button>

		<!-- Resize handles: top edge shifts the start, bottom edge changes the duration -->
		<div
			v-if="canResize"
			class="dr-resize-handle absolute inset-x-0 top-0 h-2 cursor-ns-resize opacity-0 transition-opacity group-hover:opacity-100 after:absolute after:inset-x-3 after:top-[2px] after:h-[2px] after:rounded-full after:bg-current after:opacity-40 after:content-['']"
			title="Drag to change start time"
			@pointerdown.stop.prevent="onResizeStart($event, 'top')"
			@click.stop
		/>
		<div
			v-if="canResize"
			class="dr-resize-handle absolute inset-x-0 bottom-0 h-2 cursor-ns-resize opacity-0 transition-opacity group-hover:opacity-100 after:absolute after:inset-x-3 after:bottom-[2px] after:h-[2px] after:rounded-full after:bg-current after:opacity-40 after:content-['']"
			title="Drag to change duration"
			@pointerdown.stop.prevent="onResizeStart($event, 'bottom')"
			@click.stop
		/>
	</div>
</template>

<script setup lang="ts">
	import { computed, ref } from 'vue';
	import DRIcon from './DRIcon.vue';
	import { hexAlpha, resolveCategory } from '@/utils/dailyRoutines/categoryMap';
	import { fmtTime } from '@/utils/dailyRoutines/dateHelpers';
	import type { RoutineEntry } from '@/types/dailyRoutine';

	const HOUR_PX = 56;
	const SNAP_MIN = 15;
	const MIN_DURATION = 15;

	import { useRoutineDrag } from '@/composable/useRoutineDrag';

	const props = defineProps<{
		entry: RoutineEntry;
		startMin: number;
		endMin: number;
		mode: 'full' | 'compact';
	}>();

	const emit = defineEmits<{
		(e: 'toggle', entry: RoutineEntry): void;
		(e: 'edit', entry: RoutineEntry): void;
		(e: 'context', payload: { entry: RoutineEntry; x: number; y: number }): void;
		(e: 'resize', payload: { entry: RoutineEntry; startMin: number; endMin: number }): void;
	}>();

	const { active, onPointerDown } = useRoutineDrag();
	const isBeingDragged = computed(
		() =>
			active.value &&
			active.value.task_id === props.entry.task_id &&
			active.value.date === props.entry.date,
	);

	function onClick() {
		if (active.value) return;
		emit('toggle', props.entry);
	}

	// ── resize (drag top/bottom edge) ──────────────────────────────────────────
	// Only full day-view blocks are resizable; stacked/compact ones are not.
	const canResize = computed(() => props.mode === 'full');
	const resizePreview = ref<{ startMin: number; endMin: number } | null>(null);
	const effStartMin = computed(() => resizePreview.value?.startMin ?? props.startMin);
	const effEndMin = computed(() => resizePreview.value?.endMin ?? props.endMin);

	function snap(min: number): number {
		return Math.round(min / SNAP_MIN) * SNAP_MIN;
	}

	function onResizeStart(e: PointerEvent, edge: 'top' | 'bottom') {
		const startY = e.clientY;
		const origStart = props.startMin;
		const origEnd = props.endMin;

		const onMove = (ev: PointerEvent) => {
			const deltaMin = ((ev.clientY - startY) / HOUR_PX) * 60;
			if (edge === 'bottom') {
				const newEnd = Math.min(24 * 60, Math.max(origStart + MIN_DURATION, snap(origEnd + deltaMin)));
				resizePreview.value = { startMin: origStart, endMin: newEnd };
			} else {
				const newStart = Math.max(0, Math.min(origEnd - MIN_DURATION, snap(origStart + deltaMin)));
				resizePreview.value = { startMin: newStart, endMin: origEnd };
			}
		};

		const onUp = () => {
			document.removeEventListener('pointermove', onMove);
			document.removeEventListener('pointerup', onUp);
			document.removeEventListener('pointercancel', onUp);
			const preview = resizePreview.value;
			resizePreview.value = null;
			if (preview && (preview.startMin !== origStart || preview.endMin !== origEnd)) {
				emit('resize', { entry: props.entry, startMin: preview.startMin, endMin: preview.endMin });
			}
		};

		document.addEventListener('pointermove', onMove);
		document.addEventListener('pointerup', onUp);
		document.addEventListener('pointercancel', onUp);
	}

	const top = computed(() => (effStartMin.value / 60) * HOUR_PX);
	const height = computed(() => Math.max(24, ((effEndMin.value - effStartMin.value) / 60) * HOUR_PX - 2));
	const isShort = computed(() => height.value < 38);

	const blockStyle = computed(() => {
		const raw = props.entry.routine_category as unknown;
		const cat = typeof raw === 'string' || raw == null
			? resolveCategory(raw as string | null | undefined)
			: (raw as { color: string });
		const padY = isShort.value ? '2px' : props.mode === 'full' ? '6px' : '4px';
		const padX = props.mode === 'full' ? '10px' : '6px';
		return {
			top: `${top.value}px`,
			height: `${height.value}px`,
			left: props.mode === 'full' ? '4px' : '3px',
			right: props.mode === 'full' ? '6px' : '3px',
			background: hexAlpha(cat.color, 0.2),
			borderLeft: `3px solid ${cat.color}`,
			padding: `${padY} ${padX}`,
			paddingRight: isShort.value ? '6px' : '24px',
			display: 'flex',
			alignItems: isShort.value ? 'center' : 'flex-start',
			flexDirection: isShort.value ? 'row' : 'column',
		};
	});

	const endTimeStr = computed(() => {
		const h = Math.floor(effEndMin.value / 60) % 24;
		const m = effEndMin.value % 60;
		return fmtTime(h, m);
	});
</script>
