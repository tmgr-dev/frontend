<template>
	<div
		class="dr-week-chip absolute cursor-pointer overflow-hidden rounded-md transition-opacity"
		:class="entry.completed ? 'opacity-50' : 'opacity-100'"
		:style="blockStyle"
		:title="`${entry.title} · ${entry.time} – ${endTimeStr}`"
		@click="$emit('toggle', entry)"
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
	</div>
</template>

<script setup lang="ts">
	import { computed } from 'vue';
	import DRIcon from './DRIcon.vue';
	import { hexAlpha } from '@/utils/dailyRoutines/categoryMap';
	import { fmtTime } from '@/utils/dailyRoutines/dateHelpers';
	import type { RoutineEntry } from '@/types/dailyRoutine';

	const HOUR_PX = 56;

	const props = defineProps<{
		entry: RoutineEntry;
		startMin: number;
		endMin: number;
		mode: 'full' | 'compact';
	}>();

	defineEmits<{
		(e: 'toggle', entry: RoutineEntry): void;
		(e: 'edit', entry: RoutineEntry): void;
	}>();

	const top = computed(() => (props.startMin / 60) * HOUR_PX);
	const height = computed(() => Math.max(24, ((props.endMin - props.startMin) / 60) * HOUR_PX - 2));
	const isShort = computed(() => height.value < 38);

	const blockStyle = computed(() => {
		const cat = props.entry.routine_category;
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
		const h = Math.floor(props.endMin / 60) % 24;
		const m = props.endMin % 60;
		return fmtTime(h, m);
	});
</script>
