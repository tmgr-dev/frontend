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

	// Helper to get preferred editor with local storage fallback
	const getPreferredEditorWithFallback = (): EditorType => {
		// First try to get from store
		const preferredEditor = store.state.user?.settings?.find(
			(setting: Record<string, string | number>) =>
				setting.key === 'preferred_editor',
		)?.value as EditorType | undefined;
		
		if (preferredEditor) {
			// If found in store, save to localStorage for future fallback
			localStorage.setItem('preferred_editor', preferredEditor);
			return preferredEditor;
		}
		
		// If not in store, try to get from localStorage
		const savedEditor = localStorage.getItem('preferred_editor') as EditorType | null;
		return savedEditor || 'markdown';
	};

	// Helper to set editor type and persist to localStorage
	const setEditorType = (type: EditorType) => {
		editorType.value = type;
		localStorage.setItem('preferred_editor', type);
	};

	interface Props {
		isModal: boolean;
		statusId?: number;
		modalProjectCategoryId?: number;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['close']);
	const route = useRoute();
	const router = useRouter();

	// Initialize with stored preference immediately rather than default
	const editorType = ref<EditorType>(getPreferredEditorWithFallback());
	const isEditorLoading = ref(true);
	const assignees = ref<number[]>([]);
	const modalTaskId = toRef(store.state, 'currentTaskIdForModal');
	const modalProjectCategoryId = computed(
		() => store.state.createTaskInProjectCategoryId,
	);
	const form = ref<Task>({
		title: '',
		description: '',
		description_json: null,
		approximately_time: 0,
		assignees: [],
		status: 'created',
		status_id: props.statusId ?? 0,
		project_category_id: props.modalProjectCategoryId ?? null,
		common_time: 0,
		is_daily_routine: false,
		order: 0,
		start_time: 0,
		user_id: store.state.user?.id || 0,
		workspace_id: undefined,
		id: undefined,
		category: 0,
		user: {
			id: store.state.user?.id || 0,
			name: store.state.user?.name || ''
		}
	});
	const taskId = computed(() => {
		const id = modalTaskId.value || route.params.id;
		return id ? Number(id) : undefined;
	});
	const statuses = ref<Status[]>();
	const categories = ref<Category[]>([]);
	const workspaceMembers = ref<WorkspaceMember[]>([]);
	const isLoading = ref(false);
	const workspaceStatuses = computed<Status[]>(
		() => store.state.workspaceStatuses as Status[],
	);

	const statusIdStr = computed({
		get: () => form.value.status_id?.toString() || '',
		set: (value: string) => {
			form.value.status_id = parseInt(value, 10);
		},
	});

	onBeforeMount(async () => {
		isEditorLoading.value = true;
		try {
			// First, get the workspaceId and preferred editor from settings
			const workspaceId = store.state.user?.settings?.find(
				(setting: Record<string, string | number>) =>
					setting.key === 'current_workspace',
			)?.value;

			// Get preferred editor from settings with proper fallback
			const preferredEditor = store.state.user?.settings?.find(
				(setting: Record<string, string | number>) =>
					setting.key === 'preferred_editor',
			)?.value as EditorType | undefined;
			
			if (preferredEditor) {
				setEditorType(preferredEditor);
			}

			// Load all required data in parallel
			const [loadedStatuses, loadedCategories, loadedWorkspaceMembers] =
				await Promise.all([
					getStatuses(),
					getCategories(),
					workspaceId ? getWorkspaceMembers(workspaceId) : [],
				]);

			statuses.value = loadedStatuses;
			categories.value = loadedCategories;
			workspaceMembers.value = loadedWorkspaceMembers;

			// Only after all data is loaded, update the task title
			await updateTaskTitle();

			// Load task data if we have a task ID
			if (taskId.value) {
				if (props.isModal) {
					history.pushState({}, '', `/${taskId.value}`);
				}

				suppressAutoSavingForOnce.value = true;
				const taskData = await getTask(taskId.value);
				
				// Ensure approximately_time is a number
				taskData.approximately_time = typeof taskData.approximately_time === 'string' 
					? parseInt(taskData.approximately_time, 10) || 0 
					: taskData.approximately_time || 0;
				
				// Before setting form value, check if we need to adjust editor type based on content
				if (taskData.description_json && !taskData.description) {
					// If task has JSON content but no markdown, use block editor
					setEditorType('block');
				} else if (taskData.description && !taskData.description_json) {
					// If task has markdown but no JSON, use markdown editor
					setEditorType('markdown');
				}
				
				form.value = taskData;

				// Update approximately_time from settings if available
				const approxTime = form.value.settings?.find(item => item.key === 'approximately_time')?.value;
				if (approxTime !== undefined) {
					form.value.approximately_time = typeof approxTime === 'string' 
						? parseInt(approxTime, 10) || 0 
						: Number(approxTime) || 0;
				}

				// Handle editor type and content conversion - this is critical
				// Make sure we use the correct editor type based on content
				if (editorType.value === 'block' && !form.value.description_json && form.value.description) {
					form.value.description_json = getBlockEditorDescription(form.value.description);
				} else if (editorType.value === 'markdown' && form.value.description_json && !form.value.description) {
					// If we're in markdown mode but only have JSON description, convert or provide a fallback
					form.value.description = form.value.description_json?.blocks?.[0]?.data?.text || '';
				}

				// Handle assignees
				if (form.value.assignees) {
					const taskAssignees = form.value.assignees as WorkspaceMember[];
					assignees.value = taskAssignees.map(assignee => assignee.id);
				} else {
					assignees.value = [];
				}
			}
		} catch (e) {
			console.error('Error loading task data:', e);
		} finally {
			// Set loading to false only after everything is done
			isEditorLoading.value = false;
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
				form.value = await stopTaskTimeCounter(taskId.value);
			}

			try {
				await deleteTask(taskId.value);

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

	const updateFormBeforeQuery = () => {
		suppressAutoSavingForOnce.value = true;
		// Ensure assignees is properly typed as Record<string, any>[] | number[]
		form.value.assignees = assignees.value;

		suppressAutoSavingForOnce.value = true;
		
		// Set up form data based on the current editor type
		if (editorType.value === 'block') {
			// When using block editor, ensure JSON is present
			if (!form.value.description_json && form.value.description) {
				form.value.description_json = getBlockEditorDescription(form.value.description);
			}
			// Clear the markdown description since we're using block editor
			form.value.description = null;
		} else {
			// When using markdown editor, ensure description content is not lost
			if (form.value.description_json && !form.value.description) {
				form.value.description = form.value.description_json?.blocks?.[0]?.data?.text || '';
			}
			// Clear the JSON description since we're using markdown editor
			form.value.description_json = null;
		}
	};

	const saveTask = async () => {
		if (!taskId.value) return;

		isLoading.value = true;
		updateFormBeforeQuery();

		try {
			suppressAutoSavingForOnce.value = true;
			form.value = await updateTask(taskId.value, form.value as Task);
			store.commit('incrementReloadTasksKey');
		} catch (e) {
			console.error(e);
		} finally {
			isLoading.value = false;
		}
	};

	const deleteCurrentTask = async () => {
		if (!taskId.value) return;

		isLoading.value = true;

		try {
			if (form.value.start_time) {
				suppressAutoSavingForOnce.value = true;
				form.value = await stopTaskTimeCounter(taskId.value);
			}

			await deleteTask(taskId.value);

			if (props.isModal) {
				emit('close');
			} else {
				router.push('/');
			}
		} catch (e) {
			console.error(e);
		} finally {
			isLoading.value = false;
		}
	};

	const toggleTimer = async () => {
		if (!taskId.value) return;

		isLoading.value = true;

		try {
			suppressAutoSavingForOnce.value = true;
			if (form.value.start_time) {
				form.value = await stopTaskTimeCounter(taskId.value);
			} else {
				form.value = await startTaskTimeCounter(taskId.value);
			}
		} catch (e) {
			console.error(e);
		} finally {
			isLoading.value = false;
		}
	};

	const [isAutoSaving] = useDebouncedAutoSave({
		formRef: form,
		fieldsToWatch: [
			'title',
			'description',
			// 'description_json', // TODO: need to fix update in this editor
			'project_category_id',
			'assignees',
			'status',
		],
		onSave: saveTask,
		delay: 2000,
		suppressDebounceForOnce: suppressAutoSavingForOnce,
	});

	useMagicKeys({
		passive: false,
		onEventFired(e) {
			if ((e.metaKey || e.ctrlKey) && e.code === 'KeyS') {
				e.preventDefault();
				saveTask();
			}
		},
	});

	// Add a function to toggle between editor types
	const toggleEditorType = () => {
		// Save current content before switching
		updateFormBeforeQuery();
		
		// Toggle and save the editor type
		const newType = editorType.value === 'markdown' ? 'block' : 'markdown';
		setEditorType(newType);
		
		// Update form data for the new editor type
		updateFormBeforeQuery();
	};
</script>

<template>
	<teleport to="title">{{ form.title }}&nbsp;</teleport>

	<div
		class="flex h-full flex-col gap-4 overflow-y-auto p-6"
		:class="[isModal ? 'md:w-[700px]' : 'container mx-auto pt-14']"
	>
		<header class="flex justify-between">
			<Select v-model="statusIdStr">
				<SelectTrigger class="w-40 border-0 bg-transparent">
					<SelectValue placeholder="status" />
				</SelectTrigger>
				<SelectContent class="border-0 bg-white dark:bg-gray-800">
					<SelectItem
						class="cursor-pointer text-gray-900 hover:bg-tmgr-light-blue hover:!text-white dark:text-gray-400"
						v-for="status in statuses"
						:value="status.id.toString()"
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

			<div>
				<AssigneesCombobox
					:assignees="workspaceMembers" 
					v-model="assignees as any"
				/>
			</div>

			<div class="ml-auto">
				<TimeCounter
					v-if="form.id"
					:form="form"
					:disabled="!form.id"
					@toggle="toggleTimer"
				/>
			</div>
		</div>

		<!-- Editor section with toggle button -->
		<div class="relative">
			<!-- Editor type toggle button -->
			<button 
				@click="toggleEditorType" 
				class="absolute right-2 top-2 z-10 rounded-md bg-gray-200 px-2 py-1 text-xs font-medium text-gray-700 hover:bg-gray-300 dark:bg-gray-700 dark:text-gray-300 dark:hover:bg-gray-600"
				title="Switch editor type"
			>
				{{ editorType === 'markdown' ? 'Switch to Block Editor' : 'Switch to Markdown' }}
			</button>

			<!-- Loading state -->
			<div v-if="isEditorLoading" class="mb-2 grow md:h-72 flex items-center justify-center animate-pulse bg-gray-100 dark:bg-gray-800 rounded">
				<div class="text-center">
					<div class="inline-block h-8 w-8 animate-spin rounded-full border-4 border-solid border-current border-r-transparent align-[-0.125em] motion-reduce:animate-[spin_1.5s_linear_infinite]" role="status">
						<span class="!absolute !-m-px !h-px !w-px !overflow-hidden !whitespace-nowrap !border-0 !p-0 ![clip:rect(0,0,0,0)]">Loading...</span>
					</div>
					<p class="mt-2 text-sm text-gray-500 dark:text-gray-400">Loading editor...</p>
				</div>
			</div>
			
			<!-- Editor components -->
			<template v-else>
				<Editor
					v-if="editorType === 'markdown'"
					v-model="form.description"
					class="mb-2 grow md:h-72"
					:class="[!isModal && 'lg:min-h-96']"
					:show-preview="!!(taskId && form.description)"
				/>

				<BlockEditor
					v-else-if="editorType === 'block'"
					v-model="form.description_json"
					placeholder="Type your description here or enter / to see commands or "
					class="mb-2 grow border px-2"
					:class="[!isModal ? 'lg:min-h-96' : 'overflow-y-scroll md:h-72']"
				/>
			</template>
		</div>

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
					@click="deleteCurrentTask"
					title="Delete"
					class="rounded bg-red-500/70 px-4 py-2 font-bold text-white outline-none transition hover:bg-red-600"
				>
					<TrashIcon class="size-5" />
				</button>
			</div>
		</footer>
	</div>
</template>
