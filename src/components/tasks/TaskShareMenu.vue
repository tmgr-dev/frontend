<template>
	<DropdownMenu v-if="task.id">
		<DropdownMenuTrigger as-child>
			<Button
				type="button"
				variant="ghost"
				size="icon"
				title="Share"
				aria-label="Share"
				data-testid="task-share-trigger"
				class="h-8 w-8 text-ink-muted hover:bg-surface-hover hover:text-ink"
			>
				<Share2 />
			</Button>
		</DropdownMenuTrigger>
		<DropdownMenuContent align="end" class="w-56">
			<TaskShareItems :task="task" :open-in-app="openInApp" />
		</DropdownMenuContent>
	</DropdownMenu>
</template>

<script>
	import TaskShareItems from '@/components/tasks/TaskShareItems.vue';
	import { Button } from '@/components/ui/button';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { Share2 } from 'lucide-vue-next';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'TaskShareMenu',
		components: {
			Button,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuTrigger,
			Share2,
			TaskShareItems,
		},
		props: {
			task: { type: Object, required: true },
			openInApp: { type: Boolean, default: false },
		},
	});
</script>
