<script setup lang="ts">
	import {
		XMarkIcon,
		TrashIcon,
		DocumentPlusIcon,
		BookmarkIcon,
	} from '@heroicons/vue/20/solid';
	import { ArrowTopRightOnSquareIcon } from '@heroicons/vue/24/outline';
	import store from 'src/store';
	import {
		computed,
		onBeforeMount,
		onMounted,
		reactive,
		ref,
		toRef,
	} from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import {
		addTaskAssignee,
		createTask as createTaskAction,
		deleteTask,
		deleteTaskAssignee,
		getTask,
		startTaskTimeCounter,
		stopTaskTimeCounter,
		Task,
		updateTask,
	} from 'src/actions/tmgr/tasks';
	import Editor from 'src/components/Editor.vue';
	import TextField from 'src/components/general/TextField.vue';
	import { getStatuses, Status } from 'src/actions/tmgr/statuses';
	import 'vue-multiselect/dist/vue-multiselect.css';
	import SettingsComponent from 'src/components/SettingsComponent.vue';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from 'src/components/ui/select';
	import { Button } from 'src/components/ui/button';
	import TimeCounter from 'src/components/TimeCounter.vue';
	import {
		getWorkspaceMembers,
		WorkspaceMember,
	} from 'src/actions/tmgr/workspaces';
	import AssigneesCombobox from 'src/components/AssigneesCombobox.vue';
	import { Category, getCategories } from 'src/actions/tmgr/categories';
	import CategoriesCombobox from 'src/components/CategoriesCombobox.vue';
	import convertToHHMM from 'src/utils/convertToHHMM';

	interface Props {
		isModal: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['close']);
	const route = useRoute();
	const router = useRouter();
	// @todo find out why v-model doesn't work with reactive
	//let form = reactive<Partial<Task>>({});
	let form = ref<Partial<Task>>({
		// @todo found out what the rudiment this is
		status: 'created',
		status_id: store.state.taskStatusId || undefined,
	});
	const assignees = ref<WorkspaceMember['id'][]>([]);
	const modalTaskId = toRef(store.state, 'currentTaskIdForModal');
	const modalProjectCategoryId = computed(
		() => store.state.createTaskInProjectCategoryId,
	);
	const projectCategoryId = computed(() =>
		form.value?.id
			? null
			: route.params.project_category_id || modalProjectCategoryId.value,
	);
	const taskId = computed(
		() => modalTaskId.value || (route.params.id as string),
	);
	const statuses = ref<Status[]>();
	const categories = ref<Category[]>([]);
	const workspaceMembers = ref<WorkspaceMember[]>([]);
	const isLoading = ref(false);
	const approximatelyEndTime = computed(() => {
		if (form.value.approximately_time && form.value.common_time) {
			const secondsLeft =
				new Date().getSeconds() +
				(form.value.approximately_time - form.value.common_time);

			return convertToHHMM(secondsLeft < 0 ? 0 : secondsLeft);
		}
	});

	onBeforeMount(async () => {
		const workspaceId = store.state.user?.settings?.find(
			(setting: Record<string, string | number>) =>
				setting.key === 'current_workspace',
		)?.value;

		try {
			const [loadedStatuses, loadedCategories, loadedWorkspaceMembers] =
				await Promise.all([
					getStatuses(),
					getCategories(),
					workspaceId && getWorkspaceMembers(workspaceId),
				]);
			statuses.value = loadedStatuses;
			categories.value = loadedCategories;
			workspaceMembers.value = loadedWorkspaceMembers;
		} catch (e) {
			console.error(e);
		}

		if (taskId.value) {
			if (props.isModal) {
				history.pushState({}, '', `/${taskId.value}`);
			}

			form.value = await getTask(+taskId.value);
			assignees.value =
				(form.value.assignees as WorkspaceMember[])?.map(
					(assignee) => assignee.id,
				) || [];
		}
	});

	onMounted(() => {
		// window.onkeydown = this.getShortcutSaveListener();
	});

	const createTask = async () => {
		form.value.assignees = assignees.value;
		form.value = await createTaskAction(form.value as Task);

		if (props.isModal) {
			history.pushState({}, '', `/${form.value.id}`);
			emit('close');
			store.commit('incrementReloadTasksKey');
		} else {
			await router.push({
				name: 'TasksEdit',
				params: {
					id: form.value.id,
				},
			});
		}
	};

	const removeTask = async () => {
		if (taskId.value) {
			if (form.value.start_time) {
				form.value = await stopTaskTimeCounter(+taskId.value);
			}

			try {
				await deleteTask(+taskId.value);

				if (props.isModal) {
					emit('close');
					store.commit('incrementReloadTasksKey');
				} else {
					await router.replace('/');
				}
			} catch (e) {
				console.error(e);
			}
		}
	};

	const saveTask = async () => {
		try {
			isLoading.value = true;
			form.value.assignees = assignees.value;
			if (!form.value.project_category_id) {
				delete form.value.project_category_id;
			}
			form.value.approximately_time =
				Number(
					form.value.settings?.find((item) => item.key === 'approximately_time')
						?.value,
				) || 0;

			form.value = await updateTask(+taskId.value, form.value as Task);
			store.commit('incrementReloadTasksKey');
		} catch (e) {
			console.error(e);
		} finally {
			isLoading.value = false;
		}
	};

	const toggleTimer = async () => {
		if (form.value.start_time) {
			form.value = await stopTaskTimeCounter(+taskId.value);
		} else {
			form.value = await startTaskTimeCounter(+taskId.value);
		}

		/*if (
			Array.isArray(form.value.checkpoints) &&
			form.value.checkpoints.length > 0
		) {
			form.value.checkpoints[form.value.checkpoints.length - 1].end =
				form.value.common_time;
		}*/

		/*if (form.value.start_time && form.value.status_id) {
			const statusCurrent = this.workspaceStatuses.find(
				(el) => el.type !== 'active',
			);
			if (statusCurrent) {
				const firstActiveStatus = this.workspaceStatuses.find(
					(el) => el.type === 'active',
				);
				if (firstActiveStatus) {
					this.form.status_id = firstActiveStatus.id;
				}
			}
		}*/
		await saveTask();
	};
</script>

<template>
	<teleport to="title">{{ form.title }}&nbsp;</teleport>

	<div
		class="flex h-full flex-col gap-4 overflow-y-auto rounded-lg p-6"
		:class="[isModal ? 'md:w-[700px]' : 'container mx-auto pt-14']"
	>
		<header class="flex justify-between">
			<Select v-model="form.status_id">
				<SelectTrigger class="w-40 border-0">
					<SelectValue placeholder="status" />
				</SelectTrigger>
				<SelectContent class="border-0 bg-white dark:bg-gray-800">
					<SelectItem
						class="cursor-pointer text-gray-900 hover:bg-tmgr-light-blue hover:!text-white dark:text-gray-400"
						v-for="status in statuses"
						:value="status.id"
						:show-check-mark="false"
					>
						<span
							class="mr-3 inline-block size-2 shrink-0 rounded-full"
							:style="{ backgroundColor: status.color }"
						/>
						{{ status.name }}
					</SelectItem>
				</SelectContent>
			</Select>

			<SettingsComponent v-if="!isModal" :form="form" />

			<button v-if="isModal" @click="emit('close')">
				<XMarkIcon
					class="size-5 fill-neutral-600 hover:fill-black dark:hover:fill-white"
				/>
			</button>
		</header>

		<TextField
			v-model="form.title"
			class="w-full"
			input-class="w-full text-lg font-bold border-0 !px-0 !bg-transparent"
			placeholder="Task name"
		/>

		<div class="grid grid-cols-2 gap-4 md:flex md:items-center">
			<CategoriesCombobox
				:categories="categories"
				v-model="form.project_category_id"
			/>

			<AssigneesCombobox :assignees="workspaceMembers" v-model="assignees" />

			<div class="ml-auto">
				<TimeCounter
					v-if="taskId"
					:init-task="form"
					:disabled="!form.id"
					@toggle="toggleTimer"
				/>
			</div>
		</div>

		<Editor
			v-model="form.description"
			class="mb-2 grow md:h-72"
			:class="[!isModal && 'lg:min-h-96']"
			:show-preview="!!taskId"
		/>

		<!--	actions	-->
		<footer ref="footer" class="shadow-top z-10 mt-auto w-full rounded-lg">
			<div class="flex justify-end gap-3 text-center">
				<a
					v-if="isModal && taskId"
					:href="`/${taskId}`"
					title="Open advanced form"
					class="mr-auto rounded bg-gray-500 px-4 py-2 font-bold text-white outline-none transition hover:bg-gray-600"
				>
					<ArrowTopRightOnSquareIcon class="size-5" />
				</a>

				<span
					v-if="approximatelyEndTime"
					:class="`text-${
						approximatelyEndTime === '00:00' ? 'red' : 'gray'
					}-500 estimated-info hidden py-2 pr-5 md:block`"
				>
					Left time: {{ approximatelyEndTime }}
				</span>

				<span class="relative inline-flex rounded-md shadow-sm">
					<button
						v-if="taskId"
						@click="saveTask"
						class="relative rounded bg-blue-500 px-4 py-2 font-bold text-white transition hover:bg-blue-700 focus:outline-none"
						type="button"
						title="save"
					>
						<!--						<svg
							v-if="isSaving"
							class="absolute left-1.5 top-2.5 inline h-5 w-5 animate-spin text-white"
							xmlns="http://www.w3.org/2000/svg"
							fill="none"
							viewBox="0 0 24 24"
						>
							<circle
								class="opacity-25"
								cx="12"
								cy="12"
								r="10"
								stroke="currentColor"
								stroke-width="4"
							/>
							<path
								class="opacity-75"
								fill="currentColor"
								d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
							/>
						</svg>-->
						<span><BookmarkIcon class="size-6" /></span>
					</button>
				</span>

				<button
					v-if="!taskId"
					@click="createTask"
					class="mb-5 rounded bg-orange-500 px-4 py-2 font-bold text-white transition hover:bg-orange-600 focus:outline-none sm:mb-0"
					type="button"
					title="Create"
				>
					<DocumentPlusIcon class="size-6" />
				</button>

				<!--		<button
					v-if="isCreatingTask"
					@click="$emit('cancelCreateTask')"
					class="mb-5 rounded bg-gray-500 py-2 px-4 font-bold text-white transition hover:bg-gray-600 focus:outline-none sm:mb-0"
					type="button"
				>
					Cancel
				</button>-->

				<button
					v-if="taskId"
					@click="removeTask"
					title="Delete"
					class="rounded bg-red-500/70 px-4 py-2 font-bold text-white outline-none transition hover:bg-red-600"
				>
					<TrashIcon class="size-5" />
				</button>
			</div>
		</footer>
	</div>
</template>
