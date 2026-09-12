<template>
	<div
		class="flex items-center gap-1.5 rounded-pill bg-surface-hover px-2.5 py-1 text-xs text-ink-subtle whitespace-nowrap"
		:title="breakdown"
	>
		<span class="material-icons text-sm leading-none">checklist</span>
		<span>{{ label }}</span>
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
				const parts = [`${s.inProgress ?? 0} in progress`, `${s.done ?? 0} done`];
				if (s.hidden) {
					parts.push(`${s.hidden} hidden`);
				}
				return `${parts.join(' · ')} — ${s.percent ?? 0}% complete`;
			},
		},
	});
</script>
