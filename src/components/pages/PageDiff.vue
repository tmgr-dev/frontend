<template>
	<div
		class="overflow-x-auto rounded-md border border-line font-mono text-xs"
		data-testid="page-diff"
	>
		<div
			v-for="(op, index) in ops"
			:key="index"
			class="flex whitespace-pre-wrap break-all px-2 py-0.5"
			:class="{
				'bg-status-done-bg text-status-done-fg': op.type === 'add',
				'bg-status-fix-bg text-status-fix-fg': op.type === 'del',
				'text-ink-muted': op.type === 'same',
			}"
		>
			<span class="mr-2 w-3 shrink-0 select-none opacity-60">{{
				op.type === 'add' ? '+' : op.type === 'del' ? '-' : ' '
			}}</span>
			<span class="min-w-0">{{ op.text }}</span>
		</div>
		<div v-if="!ops.length" class="px-2 py-3 text-center text-ink-subtle">
			Empty
		</div>
	</div>
</template>

<script lang="ts">
	import { diffLines } from '@/utils/pages/lineDiff';
	import { computed, defineComponent } from 'vue';

	export default defineComponent({
		name: 'PageDiff',
		props: {
			before: { type: String, default: '' },
			after: { type: String, default: '' },
		},
		setup(props) {
			const ops = computed(() => diffLines(props.before, props.after));
			return { ops };
		},
	});
</script>
