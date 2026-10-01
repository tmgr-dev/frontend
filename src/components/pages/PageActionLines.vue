<template>
	<section v-if="lines.length" data-testid="page-action-lines">
		<h2
			class="mb-2 text-xs font-semibold uppercase tracking-wide text-ink-subtle"
		>
			Действия
		</h2>
		<ul class="space-y-1">
			<li
				v-for="line in lines"
				:key="line.line"
				class="flex items-start justify-between gap-2"
			>
				<span class="min-w-0 break-words text-gray-700 dark:text-gray-300">{{
					line.text
				}}</span>
				<button
					type="button"
					class="shrink-0 rounded px-1.5 py-0.5 text-xs text-blue-600 hover:bg-blue-50 dark:text-blue-400 dark:hover:bg-blue-900/30"
					data-testid="action-to-task"
					@click="$emit('convert', line.text)"
				>
					→ задача
				</button>
			</li>
		</ul>
	</section>
</template>

<script lang="ts">
	import type { ActionLine } from '@/utils/pages/selection';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'PageActionLines',
		props: {
			lines: { type: Array as PropType<ActionLine[]>, default: () => [] },
		},
		emits: ['convert'],
	});
</script>
