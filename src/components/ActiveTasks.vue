<template>
	<div
		v-if="tasks && tasks.length > 0"
		class="fixed left-4 bottom-4 z-10 mr-2 flex flex-col items-start gap-3">
		<AnimatedRing
			@click="showActiveTasks = !showActiveTasks"
			class="z-10 order-last mb-1"
			:title="`${tasks.length} active ${tasks.length > 1 ? 'tasks' : 'task'}`">
			{{ tasks.length }}
		</AnimatedRing>

		<Transition name="transform-to-block">
			<div
				v-if="showActiveTasks"
				class="flex max-h-[300px] max-w-xs flex-col gap-1 overflow-y-auto overflow-x-hidden rounded-lg bg-tmgr-blue p-3">
				<div v-for="task in tasks">
					<transition mode="out-in" name="fade">
						<a
							:href="`/${task.id}`"
							@click.prevent="
								store.commit('setCurrentTaskIdForModal', task.id)
							">
							<span
								class="relative mr-5 inline-flex p-1 shadow-sm transition-colors duration-300">
								<!--								<AnimatedRing class="!absolute -left-2 -top-2 !h-5 !w-5" />-->

								<span
									class="max-w-[280px] overflow-hidden text-ellipsis whitespace-nowrap text-tmgr-blue dark:text-tmgr-gray">
									{{ task.title }}
								</span>
							</span>
						</a>
					</transition>
				</div>
			</div>
		</Transition>
	</div>
</template>

<script setup lang="ts">
	import { Task } from 'src/actions/tmgr/tasks';
	import store from 'src/store';
	import AnimatedRing from 'src/components/general/AnimatedRing.vue';
	import { ref } from 'vue';

	interface Props {
		tasks: Task[];
	}

	defineProps<Props>();
	const showActiveTasks = ref(false);
</script>
