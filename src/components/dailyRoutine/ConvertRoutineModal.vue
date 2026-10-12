<template>
	<Dialog :open="true" @update:open="(open) => !open && $emit('close')">
		<DialogContent
			class="max-w-[420px] gap-3 border-line bg-surface p-5 text-ink"
		>
			<DialogHeader>
				<DialogTitle class="text-base text-ink">Convert to task</DialogTitle>
				<DialogDescription class="truncate text-xs text-ink-subtle">
					{{ entry.title }}
				</DialogDescription>
			</DialogHeader>
			<ConvertRoutineFields :busy="busy" @convert="onConvert" />
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import type { RoutineEntry } from '@/types/dailyRoutine';
	import { defineComponent, type PropType } from 'vue';
	import ConvertRoutineFields from './ConvertRoutineFields.vue';

	export default defineComponent({
		name: 'ConvertRoutineModal',
		components: {
			ConvertRoutineFields,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogHeader,
			DialogTitle,
		},
		props: {
			entry: { type: Object as PropType<RoutineEntry>, required: true },
			busy: { type: Boolean, default: false },
		},
		emits: ['close', 'convert'],
		setup(props, { emit }) {
			function onConvert(target: {
				workspaceId: number;
				projectCategoryId: number | null;
			}) {
				emit('convert', { taskId: props.entry.task_id, ...target });
			}
			return { onConvert };
		},
	});
</script>
