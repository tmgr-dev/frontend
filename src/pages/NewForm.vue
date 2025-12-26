<script setup lang="ts">
	import {
		XMarkIcon,
		TrashIcon,
		DocumentPlusIcon,
		BookmarkIcon,
	} from '@heroicons/vue/20/solid';
	import { ArrowTopRightOnSquareIcon } from '@heroicons/vue/24/outline';
	import store from '@/store';
	import { computed, onBeforeMount, ref, toRef, watch } from 'vue';
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
		updateTaskStatus,
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
	import { generateTaskUrl, generateWorkspaceUrl } from '@/utils/url';
	import Checkpoints from '@/components/general/Checkpoints.vue';
	import ForbiddenAccess from '@/components/ForbiddenAccess.vue';
	import Confirm from '@/components/general/Confirm.vue';

	// Helper to get preferred editor with local storage as primary source
	const getPreferredEditorWithFallback = (): EditorType => {
		// First try to get from localStorage since it's faster
		const savedEditor = localStorage.getItem('preferred_editor');
		// Normalize the saved value to handle potential case issues or malformed values
		if (savedEditor) {
			const normalizedEditor = savedEditor.toLowerCase().trim();
			// Only return valid editor types
			if (normalizedEditor === 'block' || normalizedEditor === 'markdown') {
				console.log('Using from localStorage:', normalizedEditor);
				return normalizedEditor as EditorType;
			}
			// If invalid, remove from localStorage to avoid future issues
			console.log(
				'Invalid localStorage editor value:',
				savedEditor,
				'removing it',
			);
			localStorage.removeItem('preferred_editor');
		}

		// If not in localStorage or invalid value, try to get from store
		const preferredEditor = store.state.user?.settings?.find(
			(setting: Record<string, string | number>) =>
				setting.key === 'preferred_editor',
		)?.value as EditorType | undefined;

		if (preferredEditor) {
			const normalizedEditor = String(preferredEditor).toLowerCase().trim();

			// Only use valid editor types
			if (normalizedEditor === 'block' || normalizedEditor === 'markdown') {
				// If found in store, save to localStorage for future fallback
				console.log('Using from store settings:', normalizedEditor);
				localStorage.setItem('preferred_editor', normalizedEditor);
				return normalizedEditor as EditorType;
			}
		}

		// Default to markdown if no preference is found anywhere
		console.log('No valid editor preference found, defaulting to markdown');
		return 'markdown';
	};

	// Helper to set editor type and persist to localStorage
	const setEditorType = (type: EditorType) => {
		// Normalize the type to ensure consistency
		const normalizedType = type.toLowerCase().trim() as EditorType;
		if (normalizedType !== 'block' && normalizedType !== 'markdown') {
			console.error('Invalid editor type:', type);
			return;
		}

		editorType.value = normalizedType;
		console.log('Setting editor type to:', normalizedType);
		localStorage.setItem('preferred_editor', normalizedType);
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
		project_category_id: modalProjectCategoryId.value ?? null,
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
			name: store.state.user?.name || '',
		},
		checkpoints: [],
	});
	const taskId = computed(() => {
		// Handle all URL formats: '/19', '/workspace/tasks/19', '/:workspace_code/:category_code/:task_number'
		const id =
			modalTaskId.value ||
			route.params.id ||
			route.params.task_id ||
			route.params.taskId;
		return id ? Number(id) : undefined;
	});
	const statuses = ref<Status[]>();
	const categories = ref<Category[]>([]);
	const workspaceMembers = ref<WorkspaceMember[]>([]);
	const isLoading = ref(false);
	const checkpointUpdateKey = ref(0);
	const isCheckpointsExpanded = ref(false);
	const workspaceStatuses = computed<Status[]>(
		() => store.state.workspaceStatuses as Status[],
	);
	const permissionDenied = ref(false);

	const statusIdStr = computed({
		get: () => form.value.status_id?.toString() || '',
		set: (value: string) => {
			form.value.status_id = parseInt(value, 10);
		},
	});

	// Toggle checkpoints expanded state
	const toggleCheckpointsExpanded = () => {
		isCheckpointsExpanded.value = !isCheckpointsExpanded.value;
	};

	// Add a checkpoint to the task
	const addCheckpoint = () => {
		const { value: formValue } = form;
		if (!formValue.checkpoints) {
			formValue.checkpoints = [];
		}
		const timeSinceStartTime = !formValue.start_time
			? 0
			: Math.floor(
					(new Date().getTime() -
						new Date().setTime(formValue.start_time * 1000)) /
						1000,
			  );
		const currentTime = formValue.common_time + timeSinceStartTime;
		if (formValue.checkpoints.length > 0) {
			const prevCheckpointIndex = formValue.checkpoints.length - 1;
			formValue.checkpoints[prevCheckpointIndex].end = currentTime;
		}
		formValue.checkpoints.push({
			description: 'New one',
			start: currentTime,
			end: currentTime,
			checked: false,
			inputType: 'text',
		});
		checkpointUpdateKey.value++;
	};

	// Update seconds for the last checkpoint
	const updateSeconds = (seconds: number) => {
		if (!form.value.checkpoints || form.value.checkpoints.length === 0) {
			return;
		}
		form.value.checkpoints[form.value.checkpoints.length - 1].end = seconds;
	};

	onBeforeMount(async () => {
		// Initialize editor immediately from localStorage with better validation
		try {
			// Set the editor type using the helper to ensure consistency
			editorType.value = getPreferredEditorWithFallback();
			console.log('Editor initialized to:', editorType.value);
		} catch (err) {
			console.error('Error initializing editor preference:', err);
			editorType.value = 'markdown'; // Fallback to markdown in case of errors
		}

		// We can stop loading the editor right away since we have a value
		isEditorLoading.value = false;

		try {
			// First, get the workspaceId and preferred editor from settings
			const workspaceId = store.state.user?.settings?.find(
				(setting: Record<string, string | number>) =>
					setting.key === 'current_workspace',
			)?.value;

			// Get preferred editor from settings for future use
			// If settings value is different from localStorage, we'll update localStorage
			const preferredEditor = store.state.user?.settings?.find(
				(setting: Record<string, string | number>) =>
					setting.key === 'preferred_editor',
			)?.value as EditorType | undefined;

			if (preferredEditor) {
				// Normalize the server value
				const normalizedServerEditor =
					typeof preferredEditor === 'string'
						? preferredEditor.toLowerCase().trim()
						: String(preferredEditor).toLowerCase().trim();

				// Only update if it's a valid type and different from current
				if (
					(normalizedServerEditor === 'block' ||
						normalizedServerEditor === 'markdown') &&
					normalizedServerEditor !== editorType.value
				) {
					console.log(
						'Editor preference in store differs from localStorage, updating to:',
						normalizedServerEditor,
					);
					setEditorType(normalizedServerEditor as EditorType);
				}
			}

			// Check if we have a new task with checkpoints in localStorage when creating a new task
			if (!taskId.value && modalProjectCategoryId.value) {
				const savedTaskData = localStorage.getItem('newTaskWithCheckpoints');
				if (savedTaskData) {
					try {
						const parsedData = JSON.parse(savedTaskData);
						form.value = { ...form.value, ...parsedData };

						// Initialize assignees if present
						if (form.value.assignees && form.value.assignees.length) {
							assignees.value = form.value.assignees.map((item) => {
								if (typeof item === 'object') {
									return item.id;
								}
								return item;
							});
						}

						// Clear the saved data to prevent it from being used again
						localStorage.removeItem('newTaskWithCheckpoints');
					} catch (e) {
						console.error('Error parsing saved task data:', e);
						localStorage.removeItem('newTaskWithCheckpoints');
					}
				}
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

			// Set first status as default for new tasks if no specific status was provided
			if (
				!taskId.value &&
				!form.value.status_id &&
				statuses.value?.length > 0
			) {
				form.value.status_id = statuses.value[0].id;
			}

			// Only after all data is loaded, update the task title
			await updateTaskTitle();

			// Load task data if we have a task ID
			if (taskId.value) {
				if (props.isModal) {
					// Update URL if in modal
					const currentWorkspaceId = store.state.user?.settings?.find(
						(setting: Record<string, any>) =>
							setting.key === 'current_workspace',
					)?.value;

					const currentWorkspace = (store.state.workspaces || []).find(
						(workspace: Record<string, any>) =>
							Number(workspace.id) === Number(currentWorkspaceId),
					);

					// Only update URL if we have a workspace and we're in modal mode
					if (currentWorkspace) {
						const url = generateTaskUrl(taskId.value, currentWorkspace, null);
						if (url && url !== '/') {
							history.replaceState({}, '', url);
							store.state.urlManuallyChanged = true;
						}
					}
				}

				suppressAutoSavingForOnce.value = true;
				const taskData = await getTask(taskId.value);

				// Ensure approximately_time is a number
				taskData.approximately_time =
					typeof taskData.approximately_time === 'string'
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

				// Initialize checkpoints array if not present
				if (!form.value.checkpoints) {
					form.value.checkpoints = [];
				}

				// Update approximately_time from settings if available
				const approxTime = form.value.settings?.find(
					(item) => item.key === 'approximately_time',
				)?.value;
				if (approxTime !== undefined) {
					form.value.approximately_time =
						typeof approxTime === 'string'
							? parseInt(approxTime, 10) || 0
							: Number(approxTime) || 0;
				}

				// Handle editor type and content conversion - this is critical
				// Make sure we use the correct editor type based on content
				if (
					editorType.value === 'block' &&
					!form.value.description_json &&
					form.value.description
				) {
					form.value.description_json = getBlockEditorDescription(
						form.value.description,
					);
				} else if (
					editorType.value === 'markdown' &&
					form.value.description_json &&
					!form.value.description
				) {
					// If we're in markdown mode but only have JSON description, convert or provide a fallback
					form.value.description =
						form.value.description_json?.blocks?.[0]?.data?.text || '';
				}

				// Handle assignees
				if (form.value.assignees) {
					const taskAssignees = form.value.assignees as WorkspaceMember[];
					assignees.value = taskAssignees.map((assignee) => assignee.id);
				} else {
					assignees.value = [];
				}
			}
		} catch (e: any) {
			console.error('Error loading task data:', e);
			if (e.response?.status === 403) {
				permissionDenied.value = true;
			}
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

			// Set current task ID in the store to transition to edit mode
			if (form.value.id) {
				store.commit('setCurrentTaskIdForModal', form.value.id);
			}

			// Get current workspace
			const currentWorkspaceId = store.state.user?.settings?.find(
				(setting: Record<string, any>) => setting.key === 'current_workspace',
			)?.value;

			const currentWorkspace = (store.state.workspaces || []).find(
				(workspace: Record<string, any>) => workspace.id === currentWorkspaceId,
			);

			const category =
				form.value.category && typeof form.value.category === 'object'
					? form.value.category
					: null;

			if (props.isModal) {
				const url = generateTaskUrl(
					form.value.id as number,
					currentWorkspace,
					category,
				);
				if (url && url !== '/') {
					history.replaceState({}, '', url);
					store.state.urlManuallyChanged = true;
				}
			} else {
				const url = generateTaskUrl(
					form.value.id as number,
					currentWorkspace,
					category,
				);
				if (url && url !== '/') {
					await router.push(url);
				} else {
					console.error('Invalid URL generated for task navigation');
				}
			}

			// Trigger reload of tasks list
			store.commit('incrementReloadTasksKey');
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
					// Get current workspace
					const currentWorkspaceId = store.state.user?.settings?.find(
						(setting: Record<string, any>) =>
							setting.key === 'current_workspace',
					)?.value;

					const currentWorkspace = (store.state.workspaces || []).find(
						(workspace: Record<string, any>) =>
							workspace.id === currentWorkspaceId,
					);

					await router.replace(generateWorkspaceUrl('list', currentWorkspace));
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
				form.value.description_json = getBlockEditorDescription(
					form.value.description,
				);
			}
			// Clear the markdown description since we're using block editor
			form.value.description = null;
		} else {
			// When using markdown editor, ensure description content is not lost
			if (form.value.description_json && !form.value.description) {
				form.value.description =
					form.value.description_json?.blocks?.[0]?.data?.text || '';
			}
			// Clear the JSON description since we're using markdown editor
			form.value.description_json = null;
		}
	};

	const saveTask = async () => {
		if (!taskId.value && !form.value.id) return;

		isLoading.value = true;
		updateFormBeforeQuery();

		try {
			// Set suppressAutoSavingForOnce to true before saving to prevent auto-save during manual save
			suppressAutoSavingForOnce.value = true;

			// Cancel any pending auto-saves
			if (typeof cancelPendingAutoSave === 'function') {
				cancelPendingAutoSave();
			}

			const id = taskId.value || (form.value.id as number);
			form.value = await updateTask(id, form.value as Task);
			store.commit('incrementReloadTasksKey');

			// Ensure no auto-save will happen after this manual save
			// We need to set this after the save operation completes
			suppressAutoSavingForOnce.value = true;
		} catch (e) {
			console.error(e);
		} finally {
			isLoading.value = false;
		}
	};

	const deleteCurrentTask = async () => {
		if (!taskId.value && !form.value.id) return;

		isLoading.value = true;

		try {
			const id = taskId.value || (form.value.id as number);
			if (form.value.start_time) {
				suppressAutoSavingForOnce.value = true;
				form.value = await stopTaskTimeCounter(id);
			}

			await deleteTask(id);

			if (props.isModal) {
				emit('close');
				store.commit('incrementReloadTasksKey');
			} else {
				router.push('/');
			}
		} catch (e) {
			console.error(e);
		} finally {
			isLoading.value = false;
		}
	};

	const backlogStatusChangeConfirm = ref<{
		taskId: number;
	} | null>(null);

	const toggleTimer = async () => {
		if (!taskId.value && !form.value.id) return;

		const isTimerRunning = form.value.start_time && form.value.start_time > 0;

		if (isTimerRunning) {
			isLoading.value = true;
			try {
				suppressAutoSavingForOnce.value = true;
				const id = taskId.value || (form.value.id as number);
				form.value = await stopTaskTimeCounter(id);
				store.commit('incrementReloadTasksKey');
			} catch (e) {
				console.error(e);
			} finally {
				isLoading.value = false;
			}
			return;
		}

		isLoading.value = true;
		try {
			suppressAutoSavingForOnce.value = true;
			const id = taskId.value || (form.value.id as number);
			form.value = await startTaskTimeCounter(id);
			store.commit('incrementReloadTasksKey');
		} catch (e) {
			console.error(e);
		} finally {
			isLoading.value = false;
		}

		const currentStatuses = statuses.value || workspaceStatuses.value || [];
		const taskStatus = currentStatuses.find(s => s.id === form.value.status_id);
		
		if (taskStatus && taskStatus.type === 'default') {
			const activeStatus = currentStatuses.find(s => s.type === 'active');
			
			if (activeStatus) {
				backlogStatusChangeConfirm.value = {
					taskId: taskId.value || (form.value.id as number)
				};
				showBacklogStatusChangeConfirm.value = true;
			}
		}
	};

	const showBacklogStatusChangeConfirm = ref(false);

	const backlogConfirmBody = computed(() => {
		const currentStatuses = statuses.value || workspaceStatuses.value || [];
		const activeStatus = currentStatuses.find(s => s.type === 'active');
		const activeStatusName = activeStatus?.name || 'active';
		return `Task "${form.value.title}" is in backlog. Switch to "${activeStatusName}" status?`;
	});

	const handleBacklogStatusChangeConfirm = async () => {
		if (!backlogStatusChangeConfirm.value) return;
		
		const currentStatuses = statuses.value || workspaceStatuses.value || [];
		const activeStatus = currentStatuses.find(s => s.type === 'active');
		
		if (!activeStatus) {
			console.error('No active status found');
			showBacklogStatusChangeConfirm.value = false;
			backlogStatusChangeConfirm.value = null;
			return;
		}

		isLoading.value = true;
		try {
			suppressAutoSavingForOnce.value = true;
			const id = backlogStatusChangeConfirm.value.taskId;
			await updateTaskStatus(id, activeStatus.id);
			form.value.status_id = activeStatus.id;
			store.commit('incrementReloadTasksKey');
		} catch (e) {
			console.error('Failed to change status:', e);
		} finally {
			isLoading.value = false;
			showBacklogStatusChangeConfirm.value = false;
			backlogStatusChangeConfirm.value = null;
		}
	};

	// Use explicit type annotations to help TypeScript understand the types
	const autosaveResult = useDebouncedAutoSave({
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
	const isAutoSaving = autosaveResult[0];
	const cancelPendingAutoSave = autosaveResult[1] as () => void;

	useMagicKeys({
		passive: false,
		onEventFired(e) {
			if ((e.metaKey || e.ctrlKey) && e.code === 'KeyS') {
				e.preventDefault();
				saveTask();
			}
		},
	});

	const generateTaskUrlForAdvancedForm = () => {
		if (!taskId.value && !form.value.id) return '/';

		// Get current workspace
		const currentWorkspaceId = store.state.user?.settings?.find(
			(setting: Record<string, any>) => setting.key === 'current_workspace',
		)?.value;

		const currentWorkspace = (store.state.workspaces || []).find(
			(workspace: Record<string, any>) =>
				Number(workspace.id) === Number(currentWorkspaceId),
		);

		// Use the new URL format: /:workspace_code/tasks/:task_id
		const id = taskId.value || (form.value.id as number);
		if (currentWorkspace?.code) {
			return `/${currentWorkspace.code}/tasks/${id}`;
		}

		// Fallback to the old format if no workspace code is available
		return `/${id}`;
	};

	const currentUserId = computed(() => store.state.user?.id);

	const isAssignedToMe = computed(() => {
		if (!currentUserId.value) return false;
		return assignees.value.includes(currentUserId.value);
	});

	const assignToMe = () => {
		if (!currentUserId.value || isAssignedToMe.value) return;
		assignees.value = [...assignees.value, currentUserId.value];
	};

	// Watch for changes to modalProjectCategoryId and update form accordingly
	watch(
		modalProjectCategoryId,
		(newCategoryId) => {
			if (newCategoryId && !taskId.value) {
				form.value.project_category_id = newCategoryId;

				// Ensure a status is selected (use first status if none is already selected)
				if (
					!form.value.status_id &&
					statuses.value &&
					statuses.value.length > 0
				) {
					form.value.status_id = statuses.value[0].id;
				}

				// If categories are already loaded, try to update the title based on category pattern
				if (categories.value.length > 0) {
					updateTaskTitle();
				}
			}
		},
		{ immediate: true },
	);

	// Get unchecked checkpoints from the current task
	const getUncheckedCheckpoints = () => {
		if (!form.value.checkpoints || form.value.checkpoints.length === 0) {
			return [];
		}

		return form.value.checkpoints
			.filter((checkpoint) => !checkpoint.checked)
			.map((checkpoint) => ({
				...checkpoint,
				start: 0,
				end: 0,
			}));
	};

	// Create a new task with the unchecked checkpoints from the current task
	const createTaskWithCheckpoints = () => {
		const uncheckedCheckpoints = getUncheckedCheckpoints();

		// Create a copy of the current form data without ID and with reset times
		const newTaskData = {
			...form.value,
			id: undefined,
			title: `${form.value.title} (copy)`,
			common_time: 0,
			start_time: 0,
			checkpoints: uncheckedCheckpoints,
		};

		// Create a new task with this data
		store.commit('setShowCreatingTaskModal', form.value.status_id);
		store.commit('createTaskInProjectCategoryId', {
			projectCategoryId: form.value.project_category_id,
			statusId: form.value.status_id,
		});

		// Store the new task data in localStorage to be used when new task form opens
		localStorage.setItem('newTaskWithCheckpoints', JSON.stringify(newTaskData));

		// Close current task modal if we're in modal mode
		if (props.isModal) {
			emit('close');
		}
	};
</script>

<template>
	<div
		v-if="permissionDenied"
		class="flex min-h-[300px] items-center justify-center p-10"
	>
		<ForbiddenAccess :show-back-button="false" />
	</div>

	<div v-else class="new-form-container h-full">
		<teleport to="title">{{ form.title }}&nbsp;</teleport>

		<div
			class="flex h-full flex-col gap-4 overflow-y-auto p-6"
			:class="[
				isModal
					? 'max-h-[calc(100vh-40px)] md:w-[700px]'
					: 'container mx-auto pt-14',
			]"
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

				<div class="flex items-center gap-3">
					<SettingsComponent :form="form" />

					<button v-if="isModal" @click="emit('close')">
						<XMarkIcon
							class="size-5 fill-neutral-600 hover:fill-black dark:hover:fill-white"
						/>
					</button>
				</div>
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

				<div class="flex items-center gap-2">
					<AssigneesCombobox
						:assignees="workspaceMembers"
						v-model="assignees as any"
					/>
					<button
						v-if="!isAssignedToMe"
						type="button"
						@click="assignToMe"
						class="flex items-center justify-center rounded bg-blue-500/80 px-2 py-1.5 text-xs text-white hover:bg-blue-500 dark:bg-blue-600/70 dark:hover:bg-blue-600/90"
						title="Assign to me"
					>
						<span class="material-icons text-sm">person_add</span>
					</button>
				</div>

				<div class="ml-auto">
					<TimeCounter
						v-if="form.id"
						:form="form"
						:disabled="!form.id"
						@toggle="toggleTimer"
						@update:seconds="updateSeconds"
					/>
				</div>
			</div>

			<!-- Editor section with toggle button -->
			<div class="relative min-h-0 flex-1">
				<!-- Editor components - no loading state needed since we use localStorage -->
				<Editor
					v-if="editorType === 'markdown'"
					v-model="form.description"
					class="mb-2 grow md:h-72"
					:class="[!isModal ? 'lg:min-h-96' : 'min-h-[200px]']"
					:show-preview="!!(taskId && form.description)"
				/>

				<BlockEditor
					v-else-if="editorType === 'block'"
					v-model="form.description_json"
					placeholder="Type your description here or enter / to see commands or "
					class="block-editor-container mb-2 grow border px-2"
					:class="[
						!isModal
							? 'lg:min-h-96'
							: 'min-h-[200px] md:max-h-[350px] md:overflow-y-auto',
					]"
				/>

				<!-- Checkpoints section directly under editor - only visible when editing a task with checkpoints -->
				<div
					v-if="
						(taskId || form.id) &&
						!isCheckpointsExpanded &&
						form.checkpoints &&
						form.checkpoints.length > 0
					"
					class="checkpoints-wrapper mt-3 rounded border border-gray-200 bg-slate-100 py-0 transition-all duration-300 dark:border-gray-700 dark:bg-slate-900"
					:class="[isModal ? 'max-h-[320px] overflow-y-auto' : '']"
					:key="checkpointUpdateKey"
				>
					<div
						class="sticky top-0 z-20 flex items-center justify-between gap-2 border-b border-gray-200 bg-slate-100 px-3 py-3 text-sm dark:border-gray-700 dark:bg-slate-900"
					>
						<span class="font-medium"
							>Task Checkpoints ({{ form.checkpoints.length }})</span
						>
						<div class="flex items-center gap-2">
							<button
								class="cursor-pointer text-blue-500 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300"
								@click="addCheckpoint"
								title="Add new checkpoint"
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									width="20"
									height="20"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
									class="feather feather-plus-circle"
								>
									<circle cx="12" cy="12" r="10"></circle>
									<line x1="12" y1="8" x2="12" y2="16"></line>
									<line x1="8" y1="12" x2="16" y2="12"></line>
								</svg>
							</button>

							<button
								class="ml-1 cursor-pointer text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300"
								@click="toggleCheckpointsExpanded"
								title="Expand checkpoints"
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									width="20"
									height="20"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
									class="feather feather-maximize-2"
								>
									<polyline points="15 3 21 3 21 9"></polyline>
									<polyline points="9 21 3 21 3 15"></polyline>
									<line x1="21" y1="3" x2="14" y2="10"></line>
									<line x1="3" y1="21" x2="10" y2="14"></line>
								</svg>
							</button>
						</div>
					</div>
					<div class="px-3 py-2">
						<checkpoints :checkpoints="form.checkpoints || []" />
					</div>
				</div>

				<!-- Fullscreen modal for checkpoints -->
				<div
					v-if="isCheckpointsExpanded"
					class="fixed inset-0 z-50 flex items-center justify-center bg-black bg-opacity-75"
					@click.self="toggleCheckpointsExpanded"
				>
					<div
						class="h-[90%] w-[90%] max-w-4xl overflow-auto rounded-lg bg-white shadow-lg dark:bg-slate-900"
					>
						<div
							class="sticky top-0 z-20 flex items-center justify-between border-b border-gray-200 bg-white px-4 py-3 dark:border-gray-700 dark:bg-slate-900"
						>
							<h2 class="text-lg font-bold">Task Checkpoints</h2>
							<div class="flex items-center gap-3">
								<button
									class="flex cursor-pointer items-center gap-1 text-blue-500 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300"
									@click="addCheckpoint"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										width="16"
										height="16"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
										class="feather feather-plus-circle"
									>
										<circle cx="12" cy="12" r="10"></circle>
										<line x1="12" y1="8" x2="12" y2="16"></line>
										<line x1="8" y1="12" x2="16" y2="12"></line>
									</svg>
								</button>
								<button
									class="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300"
									@click="toggleCheckpointsExpanded"
									title="Close expanded view"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										width="24"
										height="24"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
										class="feather feather-x"
									>
										<line x1="18" y1="6" x2="6" y2="18"></line>
										<line x1="6" y1="6" x2="18" y2="18"></line>
									</svg>
								</button>
							</div>
						</div>
						<div class="p-4">
							<checkpoints :checkpoints="form.checkpoints || []" />
						</div>
					</div>
				</div>
			</div>

			<!--	actions	-->
			<footer ref="footer" class="shadow-top z-10 mt-auto w-full rounded-lg">
				<div class="flex justify-end gap-3 text-center">
					<a
						v-if="isModal && (taskId || form.id)"
						:href="generateTaskUrlForAdvancedForm()"
						title="Open advanced form"
						class="mr-auto rounded bg-gray-500 px-4 py-2 font-bold text-white outline-none transition hover:bg-gray-600"
					>
						<ArrowTopRightOnSquareIcon class="size-5" />
					</a>

					<button
						v-if="(taskId || form.id) && getUncheckedCheckpoints().length > 0"
						@click="createTaskWithCheckpoints"
						class="flex items-center gap-1 rounded bg-indigo-500 px-4 py-2 font-bold text-white transition hover:bg-indigo-600 focus:outline-none"
						type="button"
						title="Copy unchecked checkpoints to new task"
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							width="20"
							height="20"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"
						>
							<rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
							<path
								d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"
							></path>
						</svg>
					</button>

					<button
						v-if="
							(taskId || form.id) &&
							(!form.checkpoints || form.checkpoints.length === 0)
						"
						@click="addCheckpoint"
						class="flex items-center gap-1 rounded bg-emerald-500 px-4 py-2 font-bold text-white transition hover:bg-emerald-600 focus:outline-none"
						type="button"
						title="Add Checkpoint"
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							width="20"
							height="20"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"
							class="feather feather-check-circle"
						>
							<path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"></path>
							<polyline points="22 4 12 14.01 9 11.01"></polyline>
						</svg>
					</button>

					<span class="relative inline-flex rounded-md shadow-sm">
						<button
							v-if="taskId || form.id"
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
						v-if="!taskId && !form.id"
						@click="createTask"
						class="mb-5 rounded bg-orange-500 px-4 py-2 font-bold text-white transition hover:bg-orange-600 focus:outline-none sm:mb-0"
						type="button"
						title="Create"
					>
						<DocumentPlusIcon class="size-6" />
					</button>

					<button
						v-if="taskId || form.id"
						@click="deleteCurrentTask"
						title="Delete"
						class="rounded bg-red-500/70 px-4 py-2 font-bold text-white outline-none transition hover:bg-red-600"
					>
						<TrashIcon class="size-5" />
					</button>
				</div>
			</footer>
		</div>

		<Confirm
			v-if="showBacklogStatusChangeConfirm"
			title="Task in Backlog"
			:body="backlogConfirmBody"
			@onCancel="showBacklogStatusChangeConfirm = false; backlogStatusChangeConfirm = null"
			@onOk="handleBacklogStatusChangeConfirm"
		/>
	</div>
</template>

<style>
	/* Fix for modal windows with block editor content */
	.block-editor-container {
		/* Ensure the container expands properly but doesn't overflow */
		display: flex;
		flex-direction: column;
		min-height: 200px !important; /* Force minimum height */
		width: 100%;
		padding: 0 !important;
		position: relative;
		overflow: visible !important; /* Ensure toolbar and buttons can appear outside */
	}

	/* Ensure editor is given enough space in modal */
	.block-editor-container .editorjs {
		flex: 1;
		min-height: 500px !important; /* Minimum height for the actual editor */
		width: 100% !important;
		box-sizing: border-box;
		position: relative !important; /* Ensure proper positioning context */
	}

	/* Improve list display in modal windows */
	.block-editor-container .ce-block {
		max-width: 100%;
		padding: 0 4px;
		box-sizing: border-box;
	}

	/* Fix for modals to ensure content is visible and scrollable */
	.new-form-container.h-full .md\:max-h-\[350px\] {
		overflow-y: auto;
		contain: content;
		position: relative;
		padding-left: 10px;
	}

	/* Fix for long words/strings in lists */
	.cdx-list__item,
	.ce-paragraph {
		word-break: break-word;
		white-space: normal !important;
		max-width: 95% !important; /* Ensure text wraps properly */
	}

	/* Ensure multi-line bullet points format correctly */
	.cdx-list__item-content {
		white-space: normal;
	}

	/* Fix the placement of toolbar for small screens */
	@media (max-width: 768px) {
		.block-editor-container .editorjs {
			margin-left: 25px !important;
			width: calc(100% - 25px) !important;
		}

		.ce-toolbar__plus {
			left: -20px !important;
		}
	}

	/* Fix for the very long strings with no spaces shown in your screenshot */
	.ce-paragraph,
	.cdx-list__item,
	.cdx-checklist__item {
		overflow-wrap: break-word !important;
		word-wrap: break-word !important;
		-ms-word-break: break-all !important;
		word-break: break-word !important;
		max-width: 95% !important; /* Allow some margin */
	}

	/* Fix for z-index issues with toolbar in modal */
	.ce-toolbar {
		z-index: 10 !important;
	}
</style>
