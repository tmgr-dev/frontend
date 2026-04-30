<template>
	<div class="inline-flex items-center gap-0.5 rounded-card border border-line bg-surface-sunken p-0.5">
		<button
			v-for="v in views"
			:key="v.id"
			type="button"
			:title="v.label"
			:class="[
				'inline-flex items-center gap-1.5 rounded-[calc(var(--radius-lg)-2px)] px-2.5 py-1.5 text-xs font-medium transition-colors',
				model === v.id
					? 'bg-surface text-ink shadow-tmgr-xs'
					: 'text-ink-subtle hover:text-ink',
			]"
			@click="model = v.id"
		>
			<DRIcon :name="v.icon" :size="13" stroke="currentColor" />
			<span v-if="!compact">{{ v.label }}</span>
		</button>
	</div>
</template>

<script setup lang="ts">
	import DRIcon from './DRIcon.vue';
	import type { ViewId } from '@/types/dailyRoutine';

	interface View {
		id: ViewId;
		label: string;
		icon: string;
	}

	const views: View[] = [
		{ id: 'list', label: 'List', icon: 'list' },
		{ id: 'day', label: 'Day', icon: 'day' },
		{ id: 'week', label: 'Week', icon: 'week' },
		{ id: 'month', label: 'Month', icon: 'month' },
		{ id: 'year', label: 'Year', icon: 'year' },
	];

	defineProps<{ compact?: boolean }>();
	const model = defineModel<ViewId>({ required: true });
</script>
