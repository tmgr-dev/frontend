<template>
	<div
		data-tauri-drag-region
		class="flex items-center gap-1.5 rounded-pill bg-surface-hover px-2.5 py-1 text-xs whitespace-nowrap text-ink-subtle"
		:title="breakdown"
	>
		<span data-tauri-drag-region class="material-icons text-sm leading-none"
			>checklist</span
		>
		<span data-tauri-drag-region>{{ label }}</span>
	</div>
</template>

<script>
	import { t } from '@/i18n';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'BoardTaskCount',
		props: {
			summary: {
				type: Object,
				required: true,
			},
		},
		computed: {
			label() {
				const total = this.summary?.total ?? 0;
				return t('board.taskCount.label', total);
			},
			breakdown() {
				const s = this.summary || {};
				const parts = [
					t('board.taskCount.inProgress', { n: s.inProgress ?? 0 }),
					t('board.taskCount.done', { n: s.done ?? 0 }),
				];
				if (s.hidden) {
					parts.push(t('board.taskCount.hidden', { n: s.hidden }));
				}
				return t('board.taskCount.summary', {
					parts: parts.join(' · '),
					percent: s.percent ?? 0,
				});
			},
		},
	});
</script>
