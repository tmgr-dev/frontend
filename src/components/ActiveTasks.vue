<template>
	<div class="fixed left-2 bottom-2 z-10 mr-2">
		<div v-for="task in tasks" class="mb-5 inline-block">
			<transition mode="out-in" name="fade">
				<a
					v-if="task.id !== currentOpenedTaskId"
					:href="`/${task.id}/edit`"
					@click.prevent="store.commit('currentTaskIdForModal', task.id)"
				>
					<span
						class="relative mr-5 inline-flex rounded-md bg-gray-200 p-2 shadow-sm transition-colors duration-300 dark:bg-gray-800"
					>
						<span class="absolute top-0 left-0 -mt-2 -ml-2 flex h-5 w-5">
							<span
								class="absolute inline-flex h-full w-full animate-ping rounded-full bg-blue-400 opacity-75"
							/>
							<span
								class="relative inline-flex h-5 w-5 rounded-full bg-green-500"
							/>
						</span>

						<span
							class="w-32 overflow-hidden text-ellipsis whitespace-nowrap text-tmgr-blue dark:text-tmgr-gray"
						>
							{{ task.title }}
						</span>
					</span>
				</a>
			</transition>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { Task } from 'src/actions/tmgr/tasks';
	import store from 'src/store';
	import { computed } from 'vue';

	interface Props {
		tasks: Task[];
	}

	const props = defineProps<Props>();
	const currentOpenedTaskId = computed(() => store.getters.currentOpenedTaskId);
</script>
