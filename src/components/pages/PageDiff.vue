<template>
	<div
		class="overflow-x-auto rounded-md border border-gray-200 font-mono text-xs dark:border-gray-700"
		data-testid="page-diff"
	>
		<div
			v-for="(op, index) in ops"
			:key="index"
			class="flex whitespace-pre-wrap break-all px-2 py-0.5"
			:class="{
				'bg-green-50 text-green-900 dark:bg-green-900/20 dark:text-green-200':
					op.type === 'add',
				'bg-red-50 text-red-900 dark:bg-red-900/20 dark:text-red-200':
					op.type === 'del',
				'text-gray-700 dark:text-gray-300': op.type === 'same',
			}"
		>
			<span class="mr-2 w-3 shrink-0 select-none opacity-60">{{
				op.type === 'add' ? '+' : op.type === 'del' ? '-' : ' '
			}}</span>
			<span class="min-w-0">{{ op.text }}</span>
		</div>
		<div
			v-if="!ops.length"
			class="px-2 py-3 text-center text-gray-500 dark:text-gray-400"
		>
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
