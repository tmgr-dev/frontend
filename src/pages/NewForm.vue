<script setup lang="ts">
	import {
		XMarkIcon,
		TrashIcon,
		DocumentPlusIcon,
		BookmarkIcon,
	} from '@heroicons/vue/20/solid';
	import { ArrowTopRightOnSquareIcon } from '@heroicons/vue/24/outline';
	import store from '@/store';
	import { computed, onBeforeMount, ref, toRef } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import {
		createTask as createTaskAction,
		deleteTask,
		getTask,
		getTasksIndexes,
		startTaskTimeCounter,
		stopTaskTimeCounter,
		Task,
		updateTask,
	} from '@/actions/tmgr/tasks';
	import BlockEditor from '@/components/BlockEditor.vue';
	import TextField from '@/components/general/TextField.vue';
	import { getStatuses, Status } from '@/actions/tmgr/statuses';
	import SettingsComponent from '@/components/SettingsComponent.vue';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
	import { Button } from '@/components/ui/button';
	import TimeCounter from '@/components/TimeCounter.vue';
	import {
		getWorkspaceMembers,
		WorkspaceMember,
	} from '@/actions/tmgr/workspaces';
	import AssigneesCombobox from '@/components/AssigneesCombobox.vue';
	import { Category, getCategories } from '@/actions/tmgr/categories';
	import CategoriesCombobox from '@/components/CategoriesCombobox.vue';
	import Editor from '@/components/Editor.vue';
	import { EditorType } from '@/types';
	import { getBlockEditorDescription } from '@/utils/editor';
	import { titlePatternHandler } from '@/utils/titlePatternHandler.ts';
	import { useDebouncedAutoSave } from '@/composable/useDebouncedAutoSave.ts';
	import { useMagicKeys } from '@vueuse/core';

	interface Props {
		isModal: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['close']);
	const route = useRoute();
	const router = useRouter();

	const editorType = ref<EditorType>('markdown');
	const assignees = ref<WorkspaceMember['id'][]>([]);
	const modalTaskId = toRef(store.state, 'currentTaskIdForModal');
	const modalProjectCategoryId = computed(
		() => store.state.createTaskInProjectCategoryId,
	);
	// @todo find out why v-model doesn't work with reactive
	//let form = reactive<Partial<Task>>({});
	let form = ref<Partial<Task>>({
		// @todo found out what the rudiment this is
		status: 'created',
		status_id: store.state.taskStatusId || undefined,
		project_category_id:
			+route.params.project_category_id || modalProjectCategoryId.value,
	});
	const taskId = computed(
		() => modalTaskId.value || (route.params.id as string) || form.value?.id,
	);
	const statuses = ref<Status[]>();
	const categories = ref<Category[]>([]);
	const workspaceMembers = ref<WorkspaceMember[]>([]);
	const isLoading = ref(false);
	const workspaceStatuses = computed<Status[]>(
		() => store.state.workspaceStatuses as Status[],
	);

	onBeforeMount(async () => {
		const workspaceId = store.state.user?.settings?.find(
			(setting: Record<string, string | number>) =>
				setting.key === 'current_workspace',
		)?.value;

		editorType.value =
			store.state.user?.settings?.find(
				(setting: Record<string, string | number>) =>
					setting.key === 'preferred_editor',
			)?.value || editorType.value;

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

		await updateTaskTitle();

		if (taskId.value) {
			if (props.isModal) {
				history.pushState({}, '', `/${taskId.value}`);
			}

			suppressAutoSavingForOnce.value = true;
			form.value = await getTask(+taskId.value);

			console.log(
				form.value.settings?.find((item) => item.key === 'approximately_time'),
			);
			form.value.approximately_time =
				Number(
					form.value.settings?.find((item) => item.key === 'approximately_time')
						?.value,
				) || 0;

			if (
				editorType.value === 'block' &&
				!form.value.description_json &&
				form.value.description
			) {
				form.value.description_json = getBlockEditorDescription(
					form.value.description,
				);
			}

			assignees.value =
				(form.value.assignees as WorkspaceMember[])?.map(
					(assignee) => assignee.id,
				) || [];
		}
	});

	const updateTaskTitle = async () => {
		if (categories.value.length > 0 && form.value.project_category_id) {
			const currentCategory = categories.value.find(
				(category) => category.id === form.value.project_category_id,
			);
			const taskNamePatternSetting = currentCategory?.settings?.find(
				(setting) => setting.key === 'task_name_pattern_date&time',
			);
			if (!taskNamePatternSetting || !currentCategory) return;

			try {
				const indexes = await getTasksIndexes(currentCategory.id);
				form.value.title = titlePatternHandler(
					taskNamePatternSetting.value,
					new Map(Object.entries(indexes)),
				);
			} catch (e) {
				console.error(e);
			}
		}
	};

	const createTask = async () => {
		updateFormBeforeQuery();

		try {
			suppressAutoSavingForOnce.value = true;
			form.value = await createTaskAction(form.value as Task);

			if (props.isModal) {
				history.pushState({}, '', `/${form.value.id}`);
			} else {
				await router.push({
					name: 'TasksEdit',
					params: {
						id: form.value.id,
					},
				});
			}
		} catch (e) {
			console.error(e);
		}
	};

	const removeTask = async () => {
		if (taskId.value) {
			if (form.value.start_time) {
				suppressAutoSavingForOnce.value = true;
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

	const suppressAutoSavingForOnce = ref(false);

	const saveTask = async () => {
		if (!form.value.id) return;

		isLoading.value = true;
		updateFormBeforeQuery();

		try {
			suppressAutoSavingForOnce.value = true;
			form.value.approximately_time =
				Number(
					form.value.settings?.find((item) => item.key === 'approximately_time')
						?.value,
				) || 0;

			suppressAutoSavingForOnce.value = true;
			form.value = await updateTask(+taskId.value, form.value as Task);
			store.commit('incrementReloadTasksKey');
		} catch (e) {
			console.error(e);
		} finally {
			isLoading.value = false;
		}
	};

	// Modify the useDebouncedAutoSave call:
	const [isAutoSaving] = useDebouncedAutoSave({
		formRef: form,
		fieldsToWatch: [
			'title',
			'description',
			'description_json',
			'project_category_id',
			'assignees',
			'status',
		],
		onSave: saveTask,
		delay: 2000,
		suppressDebounceForOnce: suppressAutoSavingForOnce,
	});

	const updateFormBeforeQuery = () => {
		suppressAutoSavingForOnce.value = true;
		form.value.assignees = assignees.value;

		suppressAutoSavingForOnce.value = true;
		if (editorType.value === 'block') {
			form.value.description = null;
		} else {
			form.value.description_json = null;
		}
	};

	const toggleTimer = async () => {
		suppressAutoSavingForOnce.value = true;
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

		if (form.value.start_time && form.value.status_id) {
			const statusCurrent = workspaceStatuses.value.find(
				(el) => el.type !== 'active',
			);
			if (statusCurrent) {
				const firstActiveStatus = workspaceStatuses.value.find(
					(el) => el.type === 'active',
				);
				if (firstActiveStatus) {
					suppressAutoSavingForOnce.value = true;
					form.value.status_id = firstActiveStatus.id;
				}
			}
		}
		await saveTask();
	};

	useMagicKeys({
		passive: false,
		onEventFired(e) {
			if ((e.metaKey || e.ctrlKey) && e.code === 'KeyS') {
				e.preventDefault();
				saveTask();
			}
		},
	});
</script>

<template>
	<teleport to="title">{{ form.title }}&nbsp;</teleport>

	<div
		class="flex h-full flex-col gap-4 overflow-y-auto p-6"
		:class="[isModal ? 'md:w-[700px]' : 'container mx-auto pt-14']"
	>
		<header class="flex justify-between">
			<Select v-model="form.status_id">
				<SelectTrigger class="w-40 border-0 bg-transparent">
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
				@update:model-value="
					() => {
						if (!form.title) {
							updateTaskTitle();
						}
					}
				"
			/>

			<AssigneesCombobox :assignees="workspaceMembers" v-model="assignees" />

			<div class="ml-auto">
				<TimeCounter
					v-if="form.id"
					:form="form"
					:disabled="!form.id"
					@toggle="toggleTimer"
				/>
			</div>
		</div>

		<Editor
			v-if="editorType === 'markdown'"
			v-model="form.description"
			class="mb-2 grow md:h-72"
			:class="[!isModal && 'lg:min-h-96']"
			:show-preview="taskId && form.description"
		/>

		<BlockEditor
			v-else-if="editorType === 'block'"
			v-model="form.description_json"
			placeholder="Type your description here or enter / to see commands or "
			class="mb-2 grow border px-2"
			:class="[!isModal ? 'lg:min-h-96' : 'overflow-y-scroll md:h-72']"
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

				<span class="relative inline-flex rounded-md shadow-sm">
					<button
						v-if="taskId"
						@click="saveTask()"
						class="relative w-14 rounded bg-blue-500 px-4 py-2 font-bold text-white transition hover:bg-blue-700 focus:outline-none"
						type="button"
						title="save"
					>
						<svg
							v-if="isAutoSaving || isLoading"
							class="size-6 animate-spin text-white"
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
						</svg>

						<BookmarkIcon v-else class="size-6" />
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
