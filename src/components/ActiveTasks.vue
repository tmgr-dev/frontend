<template>
	<div
		v-if="tasks?.length > 0"
		class="fixed right-4 top-4 z-10 flex flex-col items-end gap-3"
	>
		<DropdownMenu>
			<DropdownMenuTrigger>
				<AnimatedRing
					@click="showActiveTasks = !showActiveTasks"
					class="z-10 mb-1"
					:title="`${tasks.length} active ${
						tasks.length > 1 ? 'tasks' : 'task'
					}`"
				>
					{{ tasks.length }}
				</AnimatedRing>
			</DropdownMenuTrigger>

			<DropdownMenuContent class="mr-4 mt-1">
				<DropdownMenuItem v-for="task in tasks">
					<a
						:href="`/${task.id}`"
						@click.prevent="store.commit('setCurrentTaskIdForModal', task.id)"
					>
						<span
							class="relative mr-5 inline-flex p-1 shadow-sm transition-colors duration-300"
						>
							<span
								class="max-w-64 truncate text-secondary text-tmgr-blue dark:text-tmgr-gray"
							>
								{{ task.title }}
							</span>
						</span>
					</a>
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	</div>
</template>

<script setup lang="ts">
	import { Task } from '@/actions/tmgr/tasks';
	import store from '@/store';
	import AnimatedRing from '@/components/general/AnimatedRing.vue';
	import { ref } from 'vue';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';

	interface Props {
		tasks: Task[];
	}

	defineProps<Props>();
	const showActiveTasks = ref(false);
</script>
