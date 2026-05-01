<template>
	<div class="flex flex-1 flex-col gap-4 overflow-auto">
		<section
			v-if="unscheduled.length || dragActive"
			class="rounded-card transition-colors"
			:class="hoverKey === `unsched:${todayIso}` ? 'bg-brand/10 ring-1 ring-brand/40' : ''"
			data-dr-drop
			data-dr-kind="unscheduled"
			:data-dr-date="todayIso"
		>
			<header class="flex items-center justify-between gap-2 px-1 pb-2">
				<div class="text-[10px] font-bold uppercase tracking-wider text-ink-subtle">
					Unscheduled <span class="text-ink-subtle">{{ unscheduled.length }}</span>
				</div>
				<button
					v-if="unscheduledDoneCount > 0"
					type="button"
					class="rounded-pill border border-line bg-surface px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider text-ink-subtle hover:border-brand hover:text-brand"
					@click="$emit('archive-done-unscheduled')"
				>
					Archive done ({{ unscheduledDoneCount }})
				</button>
			</header>
			<div class="flex flex-col gap-1.5">
				<RoutineRow
					v-for="e in unscheduled"
					:key="`u-${e.task_id}-${e.date}`"
					:entry="e"
					@toggle="$emit('toggle', e)"
					@edit="$emit('edit', e)"
					@delete="$emit('delete', e)"
					@archive="$emit('archive', e)"
					@select="$emit('select', e)"
				/>
			</div>
		</section>
		<section
			class="rounded-card transition-colors"
			:class="hoverKey === `day:${todayIso}` ? 'bg-brand/10 ring-1 ring-brand/40' : ''"
			data-dr-drop
			data-dr-kind="day-cell"
			:data-dr-date="todayIso"
		>
			<header class="px-1 pb-2 text-[10px] font-bold uppercase tracking-wider text-brand">
				Today <span class="text-ink-subtle">{{ scheduled.length }}</span>
			</header>
			<div class="flex flex-col gap-1.5">
				<RoutineRow
					v-for="e in scheduled"
					:key="`t-${e.task_id}-${e.date}`"
					:entry="e"
					@toggle="$emit('toggle', e)"
					@edit="$emit('edit', e)"
					@delete="$emit('delete', e)"
					@archive="$emit('archive', e)"
					@select="$emit('select', e)"
				/>
			</div>
		</section>
	</div>
</template>

<script setup lang="ts">
	import { computed } from 'vue';
	import RoutineRow from '../RoutineRow.vue';
	import type { RoutineEntry } from '@/types/dailyRoutine';
	import { useRoutineDrag } from '@/composable/useRoutineDrag';

	const { active, hoverKey } = useRoutineDrag();
	const dragActive = computed(() => !!active.value);

	function isoToday(): string {
		const d = new Date();
		return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
	}
	const todayIso = isoToday();

	const props = defineProps<{
		entries: RoutineEntry[];
	}>();

	defineEmits<{
		(e: 'toggle', entry: RoutineEntry): void;
		(e: 'edit', entry: RoutineEntry): void;
		(e: 'delete', entry: RoutineEntry): void;
		(e: 'select', entry: RoutineEntry): void;
		(e: 'archive', entry: RoutineEntry): void;
		(e: 'archive-done-unscheduled'): void;
	}>();

	const unscheduled = computed(() =>
		props.entries.filter(e => !e.time)
	);

	const unscheduledDoneCount = computed(
		() => unscheduled.value.filter(e => e.completed).length,
	);

	const scheduled = computed(() =>
		props.entries
			.filter(e => !!e.time)
			.sort((a, b) => {
				const at = a.time ? a.time.split(':').map(Number) : [99, 99];
				const bt = b.time ? b.time.split(':').map(Number) : [99, 99];
				return at[0] * 60 + at[1] - (bt[0] * 60 + bt[1]);
			})
	);
</script>
