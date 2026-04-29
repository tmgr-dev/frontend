<template>
	<div
		class="group relative flex items-center gap-3 overflow-hidden rounded-card border bg-surface px-3.5 py-3 shadow-tmgr-xs transition-all"
		:class="[
			selected
				? 'border-brand bg-brand-bg shadow-tmgr-md'
				: 'border-line hover:border-line-strong hover:shadow-tmgr-md cursor-pointer',
		]"
		@click="$emit('select', entry)"
	>
		<span
			class="absolute left-0 top-0 bottom-0 w-[3px]"
			:style="{ background: entry.routine_category.color }"
		/>

		<button
			type="button"
			class="flex h-7 w-7 shrink-0 items-center justify-center rounded-pill border transition-colors"
			:class="
				entry.completed
					? 'border-status-done bg-status-done text-white'
					: 'border-line text-ink-subtle hover:border-status-done hover:text-status-done'
			"
			@click.stop="$emit('toggle', entry)"
		>
			<DRIcon v-if="entry.completed" name="check" :size="13" stroke="#fff" :sw="2.4" />
		</button>

		<div class="min-w-0 flex-1">
			<div
				class="truncate text-sm leading-snug"
				:class="entry.completed ? 'text-ink-subtle line-through' : 'text-ink'"
			>
				{{ entry.title }}
			</div>
			<div
				v-if="entry.time || entry.frequency !== 'NONE'"
				class="mt-1 flex items-center gap-2.5 text-2xs text-ink-subtle"
			>
				<span v-if="entry.time" class="inline-flex items-center gap-1">
					<DRIcon name="clock" :size="10" stroke="currentColor" />
					{{ entry.time }}
				</span>
				<span v-if="entry.frequency !== 'NONE'" class="inline-flex items-center gap-1">
					<DRIcon name="repeat" :size="10" stroke="currentColor" />
					{{ frequencyLabel }}
				</span>
				<span
					class="text-[10px] font-semibold uppercase tracking-wide"
					:style="{ color: entry.routine_category.color }"
				>
					{{ entry.routine_category.name }}
				</span>
			</div>
		</div>

		<button
			type="button"
			class="flex h-8 w-8 items-center justify-center rounded-pill text-ink-subtle opacity-0 transition-opacity hover:bg-surface-hover hover:text-brand group-hover:opacity-100"
			title="Edit"
			@click.stop="$emit('edit', entry)"
		>
			<DRIcon name="pencil" :size="13" stroke="currentColor" />
		</button>

		<button
			type="button"
			class="flex h-8 w-8 items-center justify-center rounded-pill text-ink-subtle opacity-0 transition-opacity hover:bg-surface-hover hover:text-brand group-hover:opacity-100"
			title="Archive"
			@click.stop="$emit('archive', entry)"
		>
			<DRIcon name="archive" :size="13" stroke="currentColor" />
		</button>

		<button
			type="button"
			class="flex h-8 w-8 items-center justify-center rounded-pill text-ink-subtle opacity-0 transition-opacity hover:bg-status-fix-bg hover:text-status-fix-fg group-hover:opacity-100"
			title="Delete"
			@click.stop="$emit('delete', entry)"
		>
			<DRIcon name="trash" :size="13" stroke="currentColor" />
		</button>
	</div>
</template>

<script setup lang="ts">
	import { computed } from 'vue';
	import DRIcon from './DRIcon.vue';
	import type { RoutineEntry } from '@/types/dailyRoutine';
	import { entryRecurrenceLabel } from '@/utils/dailyRoutines/recurrenceLabel';

	const props = defineProps<{
		entry: RoutineEntry;
		selected?: boolean;
	}>();

	defineEmits<{
		(e: 'toggle', entry: RoutineEntry): void;
		(e: 'edit', entry: RoutineEntry): void;
		(e: 'delete', entry: RoutineEntry): void;
		(e: 'select', entry: RoutineEntry): void;
		(e: 'archive', entry: RoutineEntry): void;
	}>();

	const frequencyLabel = computed(() => entryRecurrenceLabel(props.entry));
</script>
