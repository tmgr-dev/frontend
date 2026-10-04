<template>
	<div
		data-tauri-drag-region
		class="flex items-center gap-1.5 whitespace-nowrap rounded-pill bg-surface-hover px-2.5 py-1 text-xs text-ink-subtle"
		:title="breakdown"
	>
		<span data-tauri-drag-region class="material-icons text-sm leading-none"
			>checklist</span
		>
		<span data-tauri-drag-region>{{ label }}</span>
	</div>
</template>

<script>
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
				return `${total} ${total === 1 ? 'task' : 'tasks'}`;
			},
			breakdown() {
				const s = this.summary || {};
				const parts = [
					`${s.inProgress ?? 0} in progress`,
					`${s.done ?? 0} done`,
				];
				if (s.hidden) {
					parts.push(`${s.hidden} hidden`);
				}
				return `${parts.join(' · ')} — ${s.percent ?? 0}% complete`;
			},
		},
	});
</script>
