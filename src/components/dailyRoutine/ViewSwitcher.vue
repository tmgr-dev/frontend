<template>
	<div
		class="flex w-full items-center gap-0.5 rounded-card border border-line bg-surface-sunken p-0.5 sm:inline-flex sm:w-auto"
		data-tour="routines.view-switcher"
	>
		<button
			v-for="v in views"
			:key="v.id"
			type="button"
			:title="$t(v.labelKey)"
			:class="[
				'inline-flex flex-1 items-center justify-center gap-1.5 rounded-[calc(var(--radius-lg)-2px)] px-2.5 py-1.5 text-xs font-medium transition-colors sm:flex-none',
				model === v.id
					? 'bg-surface text-ink shadow-tmgr-xs'
					: 'text-ink-subtle hover:text-ink',
			]"
			@click="model = v.id"
		>
			<DRIcon :name="v.icon" :size="13" stroke="currentColor" />
			<span v-if="!compact">{{ $t(v.labelKey) }}</span>
		</button>
	</div>
</template>

<script setup lang="ts">
	import type { ViewId } from '@/types/dailyRoutine';
	import DRIcon from './DRIcon.vue';

	interface View {
		id: ViewId;
		labelKey: string;
		icon: string;
	}

	const views: View[] = [
		{ id: 'list', labelKey: 'dailyRoutine.views.list', icon: 'list' },
		{ id: 'day', labelKey: 'dailyRoutine.views.day', icon: 'day' },
		{ id: 'week', labelKey: 'dailyRoutine.views.week', icon: 'week' },
		{ id: 'month', labelKey: 'dailyRoutine.views.month', icon: 'month' },
		{ id: 'year', labelKey: 'dailyRoutine.views.year', icon: 'year' },
	];

	defineProps<{ compact?: boolean }>();
	const model = defineModel<ViewId>({ required: true });
</script>
