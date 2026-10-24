<script setup lang="ts">
	import { Task } from '@/actions/tmgr/tasks';
	import { Workspace } from '@/actions/tmgr/workspaces';
	import AnimatedRing from '@/components/general/AnimatedRing.vue';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import store from '@/store';
	import { generateTaskUrl } from '@/utils/url';
	import { computed, ref } from 'vue';

	interface Props {
		tasks?: Task[];
	}

	withDefaults(defineProps<Props>(), {
		tasks: () => [],
	});
	const showActiveTasks = ref(false);

	const currentWorkspaceId = computed(() => {
		const setting = store.state.user?.settings?.find(
			(setting: any) => setting.key === 'current_workspace',
		);
		return setting ? setting.value : null;
	});

	const currentWorkspace = computed(() => {
		return ((store.state.workspaces as Workspace[]) || []).find(
			(workspace) => workspace.id === currentWorkspaceId.value,
		);
	});

	const getTaskUrl = (task: Task) => {
		return generateTaskUrl(
			task.id as number,
			currentWorkspace.value,
			task.category && typeof task.category === 'object' ? task.category : null,
		);
	};
</script>

<template>
	<div
		v-if="tasks?.length > 0"
		data-tour="nav.active-timers"
		class="fixed right-4 bottom-4 z-10 flex flex-col items-end gap-3"
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

			<DropdownMenuContent class="mr-4 mb-1" side="top" align="end">
				<DropdownMenuItem v-for="task in tasks" :key="task.id">
					<a
						:href="getTaskUrl(task)"
						@click.prevent="store.commit('setCurrentTaskIdForModal', task.id)"
					>
						<span
							class="relative mr-5 inline-flex p-1 shadow-xs transition-colors duration-300"
						>
							<span class="max-w-64 truncate text-ink">
								{{ task.title }}
							</span>
						</span>
					</a>
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	</div>
</template>
