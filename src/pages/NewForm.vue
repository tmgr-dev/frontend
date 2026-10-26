<script setup lang="ts">
	import {
		Category,
		getCategories,
		getCategoriesOfWorkspace,
	} from '@/actions/tmgr/categories';
	import {
		isCommentAiReplyOn,
		setCommentAiReply,
	} from '@/actions/tmgr/commentAiReply';
	import {
		createAskingHelpComment,
		createComment,
	} from '@/actions/tmgr/comments';
	import {
		getCursorAgents,
		getCursorStatus,
		sendFollowUp,
	} from '@/actions/tmgr/cursor';
	import { uploadTaskFile } from '@/actions/tmgr/files';
	import {
		getCategoryGitHubStatus,
		getTaskGitActivity,
	} from '@/actions/tmgr/github';
	import { getAssignablePersonas } from '@/actions/tmgr/personas';
	import {
		getStatuses,
		getStatusesOfWorkspace,
		Status,
	} from '@/actions/tmgr/statuses';
	import {
		assignPersonaToTask,
		createTask as createTaskAction,
		deleteTask,
		getTask,
		getTasksIndexes,
		startTaskTimeCounter,
		stopTaskTimeCounter,
		Task,
		unassignPersonaFromTask,
		updateTask,
		updateTaskStatus,
	} from '@/actions/tmgr/tasks';
	import {
		getWorkspaceMembers,
		WorkspaceMember,
	} from '@/actions/tmgr/workspaces';
	import AssigneesCombobox from '@/components/AssigneesCombobox.vue';
	import CommentAttachments from '@/components/attachments/CommentAttachments.vue';
	import CategoriesCombobox from '@/components/CategoriesCombobox.vue';
	import DescriptionPreview from '@/components/description/DescriptionPreview.vue';
	import ForbiddenAccess from '@/components/ForbiddenAccess.vue';
	import Checkpoints from '@/components/general/Checkpoints.vue';
	import Confirm from '@/components/general/Confirm.vue';
	import PersonaAssigneeChips from '@/components/general/PersonaAssigneeChips.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import PluginTaskMenuItems from '@/components/plugins/PluginTaskMenuItems.vue';
	import PluginTaskSections from '@/components/plugins/PluginTaskSections.vue';
	import TaskAgentRunner from '@/components/runner/TaskAgentRunner.vue';
	import SettingsComponent from '@/components/SettingsComponent.vue';
	import AgentStatusChip from '@/components/tasks/AgentStatusChip.vue';
	import AskPersonaButton from '@/components/tasks/AskPersonaButton.vue';
	import CloudAgentApproval from '@/components/tasks/CloudAgentApproval.vue';
	import ConfirmForAgentRuns from '@/components/tasks/ConfirmForAgentRuns.vue';
	import DoneCheckbox from '@/components/tasks/DoneCheckbox.vue';
	import PomodoroBlock from '@/components/tasks/PomodoroBlock.vue';
	import TaskAgentWork from '@/components/tasks/TaskAgentWork.vue';
	import TaskAttachments from '@/components/tasks/TaskAttachments.vue';
	import TaskComments from '@/components/tasks/TaskComments.vue';
	import TaskCursorAgent from '@/components/tasks/TaskCursorAgent.vue';
	import TaskFormSkeleton from '@/components/tasks/TaskFormSkeleton.vue';
	import TaskGitActivity from '@/components/tasks/TaskGitActivity.vue';
	import TaskLinkChips from '@/components/tasks/TaskLinkChips.vue';
	import TaskPageMentions from '@/components/tasks/TaskPageMentions.vue';
	import TaskPersonChips from '@/components/tasks/TaskPersonChips.vue';
	import TaskRelations from '@/components/tasks/TaskRelations.vue';
	import TaskShareMenu from '@/components/tasks/TaskShareMenu.vue';
	import TaskTimeEntries from '@/components/tasks/TaskTimeEntries.vue';
	import TaskTimeInfo from '@/components/tasks/TaskTimeInfo.vue';
	import TimeCounter from '@/components/TimeCounter.vue';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
	import { useToast } from '@/components/ui/toast/use-toast';
	import MicButton from '@/components/voice/MicButton.vue';
	import { useDebouncedAutoSave } from '@/composable/useDebouncedAutoSave.ts';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { useModalEscHandler } from '@/composable/useModalEscHandler';
	import { usePluginTaskMenu } from '@/composable/usePluginTaskMenu';
	import { usePusher } from '@/composable/usePusher';
	import { useRunningTimerSync } from '@/composable/useRunningTimerSync';
	import { useWorkflowMode } from '@/composable/useWorkflowMode';
	import store from '@/store';
	import { tourBus } from '@/tours/events';
	import { EditorType } from '@/types';
	import type { AgentStep } from '@/types/agent';
	import type { CursorAgent } from '@/types/cursor';
	import { autoSignQuietly } from '@/utils/agentApprovalClient';
	import { agentToolLabel } from '@/utils/agentToolLabels';
	import { attachmentErrorMessage } from '@/utils/aiAttachments';
	import {
		backlogTimerPrompt,
		type BacklogTimerPrompt,
	} from '@/utils/backlogTimerPrompt';
	import { footerHeightVars } from '@/utils/bottomBar';
	import {
		getCategoryIntegrationHint,
		type CategoryIntegrationHint,
	} from '@/utils/categoryIntegrationHint';
	import { isSendEnter } from '@/utils/commentComposer';
	import { pickDefaultStatusId } from '@/utils/defaultStatus';
	import { isDesktopApp } from '@/utils/desktop';
	import { getBlockEditorDescription, sameBlocks } from '@/utils/editor';
	import {
		buildDescriptionPayload,
		EDITOR_LABELS,
		normalizeEditorType,
		resolveEditorType,
		toUpdateBody,
	} from '@/utils/editorType';
	import { createFileDragDepth, isFileDrag } from '@/utils/fileDrag';
	import { focusField } from '@/utils/focusTarget';
	import {
		applicableTaskSettings,
		hasTaskSettings,
		loadLastTaskSettings,
		saveLastTaskSettings,
	} from '@/utils/lastTaskSettings';
	import { pagesAvailable } from '@/utils/pagesTree';
	import { uploadPendingFiles } from '@/utils/pendingUploads';
	import {
		personaAssigneeIds,
		personaAssigneesOf,
		type PersonaAssignee,
	} from '@/utils/personas';
	import { normalizePriority, PRIORITIES } from '@/utils/priority';
	import { requestCache } from '@/utils/requestCache';
	import { isSaveHotkey } from '@/utils/saveHotkey';
	import {
		archiveTarget,
		DONE_DISABLED_TOOLTIP,
		doneTarget,
		isTaskArchived,
		isTaskDone,
		openTarget,
	} from '@/utils/simpleStatus';
	import { taskCreator } from '@/utils/taskCreator';
	import { loadTaskFormReferenceData } from '@/utils/taskFormReferenceData';
	import { mergeSavedTask } from '@/utils/taskSaveSnapshot';
	import { createTaskVersionGuard } from '@/utils/taskVersionGuard';
	import {
		openTaskWindow,
		setTaskWindowTitle,
		taskWindowTarget,
	} from '@/utils/taskWindow';
	import { applyTimerState } from '@/utils/timerSync';
	import {
		joinCarriedMarkdown,
		prependCarriedBlock,
		splitTitleInsert,
	} from '@/utils/titleOverflow';
	import { titlePatternHandler } from '@/utils/titlePatternHandler.ts';
	import { generateTaskUrl } from '@/utils/url';
	import {
		DocumentPlusIcon,
		TrashIcon,
		XMarkIcon,
	} from '@heroicons/vue/20/solid';
	import {
		ArrowDownTrayIcon,
		ArrowTopRightOnSquareIcon,
		CodeBracketIcon,
		FlagIcon,
		FolderIcon,
		HashtagIcon,
		LinkIcon,
		UserCircleIcon,
		UserIcon,
	} from '@heroicons/vue/24/outline';
	import { useMagicKeys } from '@vueuse/core';
	import {
		Archive as ArchiveIcon,
		ArchiveRestore,
		Bot,
		Loader2,
		MoreVertical,
		Save,
		Send,
		Sparkles,
		Upload,
	} from 'lucide-vue-next';
	import {
		computed,
		defineAsyncComponent,
		nextTick,
		onBeforeMount,
		onBeforeUnmount,
		onMounted,
		onUnmounted,
		ref,
		toRef,
		watch,
	} from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	const BlockEditor = defineAsyncComponent(
		() => import('@/components/BlockEditor.vue'),
	);
	const BlockMdEditor = defineAsyncComponent(
		() => import('@/components/BlockMdEditor.vue'),
	);
	const Editor = defineAsyncComponent(() => import('@/components/Editor.vue'));
	const EntityGraphSection = defineAsyncComponent(
		() => import('@/components/graph/EntityGraphSection.vue'),
	);

	// Helper to get preferred editor with local storage as primary source
	const getPreferredEditorWithFallback = (): EditorType => {
		// First try to get from localStorage since it's faster
		const savedEditor = localStorage.getItem('preferred_editor');
		if (savedEditor) {
			const normalizedEditor = normalizeEditorType(savedEditor);
			if (normalizedEditor) {
				console.log('Using from localStorage:', normalizedEditor);
				return normalizedEditor;
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
		const preferredEditor = normalizeEditorType(
			store.state.user?.settings?.find(
				(setting) => setting.key === 'preferred_editor',
			)?.value,
		);

		if (preferredEditor) {
			// If found in store, save to localStorage for future fallback
			console.log('Using from store settings:', preferredEditor);
			localStorage.setItem('preferred_editor', preferredEditor);
			return preferredEditor;
		}

		// Default to markdown if no preference is found anywhere
		console.log('No valid editor preference found, defaulting to markdown');
		return 'markdown';
	};

	// Helper to set editor type and persist to localStorage
	const setEditorType = (type: EditorType) => {
		const normalizedType = normalizeEditorType(type);
		if (!normalizedType) {
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
		detached?: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['close']);
	const route = useRoute();
	const router = useRouter();

	// Initialize with stored preference immediately rather than default
	const editorType = ref<EditorType>(getPreferredEditorWithFallback());
	// Legacy Editor.js tasks always open in Editor.js; tell the user when that
	// differs from what they picked in Settings.
	const showsLegacyEditorNote = computed(
		() =>
			editorType.value === 'block' &&
			getPreferredEditorWithFallback() !== 'block',
	);
	const descriptionDirty = ref(false);
	const applyResolvedEditor = (
		task: Parameters<typeof resolveEditorType>[0],
	) => {
		editorType.value = resolveEditorType(
			task,
			getPreferredEditorWithFallback(),
		);
		descriptionDirty.value = false;
	};
	const onDescriptionInput = (
		field: 'description' | 'description_json',
		value: any,
	) => {
		descriptionDirty.value = true;
		form.value[field] = value;
	};
	const isEditorLoading = ref(true);
	const assignees = ref<number[]>([]);
	const modalTaskId = toRef(store.state, 'currentTaskIdForModal');
	const modalProjectCategoryId = computed(
		() => store.state.createTaskInProjectCategoryId,
	);
	const { isFeatureEnabled, showSurface } = useFeatureToggles();
	const { simpleMode } = useWorkflowMode();
	const pagesVisible = computed(() =>
		pagesAvailable(store.getters.currentWorkspace, isFeatureEnabled('pages')),
	);

	const {
		subscribeToWorkspace,
		unsubscribeHandlerFromWorkspace,
		subscribeToUser,
		unsubscribeHandler,
	} = usePusher();
	const hasExternalUpdate = ref(false);
	const externalUpdateData = ref<Task | null>(null);
	const subscribedWorkspaceId = ref<number | null>(null);
	const pusherSubscriptionId = ref<string>('');
	const aiPending = ref(false);
	const aiPendingSteps = ref<AgentStep[]>([]);
	const isForThisTask = (e: { task_id?: number | null }) =>
		!!form.value.id && e.task_id === form.value.id;
	const subscribedUserId = ref<number | null>(null);
	const userPusherSubscriptionId = ref<string>('');
	const instanceId = `new-form-${Date.now()}-${Math.random()
		.toString(36)
		.substr(2, 9)}`;

	const hasTaskMeaningfulChanges = (current: Task, incoming: any): boolean => {
		const fieldsToCompare = [
			'title',
			'description',
			'description_json',
			'status_id',
			'project_category_id',
			'category_tasks_sequence_id',
			'expired_at',
			'approximately_time',
			'checkpoints',
			'start_time',
			'common_time',
		];
		for (const field of fieldsToCompare) {
			const currentVal = current[field as keyof Task];
			const incomingVal = incoming[field];

			let isDifferent = false;
			if (typeof currentVal === 'object' || typeof incomingVal === 'object') {
				isDifferent =
					JSON.stringify(currentVal) !== JSON.stringify(incomingVal);
			} else {
				isDifferent = String(currentVal ?? '') !== String(incomingVal ?? '');
			}

			if (isDifferent) {
				console.log(`[NewForm] Field "${field}" differs:`, {
					current: currentVal,
					incoming: incomingVal,
				});
				return true;
			}
		}
		if (
			'priority' in incoming &&
			normalizePriority(current.priority) !==
				normalizePriority(incoming.priority)
		)
			return true;
		console.log('[NewForm] No meaningful changes detected');
		return false;
	};

	const syncRemoteRelations = (task: any) => {
		if (
			'relationTypeWithTask' in task &&
			JSON.stringify(task.relationTypeWithTask) !==
				JSON.stringify(form.value.relationTypeWithTask)
		) {
			suppressAutoSavingForOnce.value = true;
			form.value.relationTypeWithTask = task.relationTypeWithTask;
		}
	};

	const handleRemoteTaskUpdate = (task: any) => {
		if (hasTaskMeaningfulChanges(form.value, task)) {
			hasExternalUpdate.value = true;
			externalUpdateData.value = task;
		}
	};
	const taskVersionGuard = createTaskVersionGuard(
		handleRemoteTaskUpdate,
		(known, incoming) =>
			!hasTaskMeaningfulChanges({ ...incoming, ...known }, incoming),
	);
	let loadTaskReferenceData:
		((taskWorkspaceId: number | null) => unknown) | null = null;

	const form = ref<Task>({
		title: '',
		description: '',
		description_json: null,
		approximately_time: 0,
		assignees: [],
		status: 'created',
		status_id: props.statusId ?? 0,
		project_category_id: modalProjectCategoryId.value ?? null,
		priority: 'medium',
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
		// If we're in task creation mode, don't use route params (they might be category ID)
		if (store.state.showCreatingTaskModal && !modalTaskId.value) {
			return undefined;
		}

		// Handle all URL formats: '/19', '/workspace/tasks/19', '/:workspace_code/:category_code/:task_number'
		const id =
			modalTaskId.value ||
			route.params.id ||
			route.params.task_id ||
			route.params.taskId ||
			route.params.task_number;
		return id ? Number(id) : undefined;
	});
	const statuses = ref<Status[]>();
	const categories = ref<Category[]>([]);
	const workspaceMembers = ref<WorkspaceMember[]>([]);
	const assignablePersonas = ref<PersonaAssignee[]>([]);
	const personaAssignees = ref<PersonaAssignee[]>([]);
	const memberAvatars = computed(() =>
		Object.fromEntries(
			workspaceMembers.value.map((m) => [m.id, !!m.has_avatar]),
		),
	);
	const isLoading = ref(false);
	const checkpointUpdateKey = ref(0);
	const isCheckpointsExpanded = ref(false);
	const checkpointsModalId = `checkpoints-${Date.now()}-${Math.random()}`;
	const { registerModal, unregisterModal } = useModalEscHandler();
	const workspaceStatuses = computed<Status[]>(
		() => store.state.workspaceStatuses as Status[],
	);

	watch(isCheckpointsExpanded, (isOpen: boolean) => {
		if (isOpen) {
			store.commit('pushModalToStack', checkpointsModalId);
			registerModal(checkpointsModalId, () => {
				isCheckpointsExpanded.value = false;
			});
		} else {
			unregisterModal(checkpointsModalId);
			store.commit('removeModalFromStack', checkpointsModalId);
		}
	});
	const descriptionWrapperRef = ref<HTMLElement | null>(null);
	const descriptionEditFor = ref<number | null>(null);
	const isDescriptionEditOpen = computed(
		() => !!form.value.id && descriptionEditFor.value === form.value.id,
	);
	const showDescriptionEditor = computed(
		() => (!taskId.value && !form.value.id) || isDescriptionEditOpen.value,
	);
	let descriptionSnapshot: Pick<Task, 'description' | 'description_json'> = {
		description: null,
		description_json: null,
	};
	const startDescriptionEdit = () => {
		descriptionSnapshot = JSON.parse(
			JSON.stringify({
				description: form.value.description ?? null,
				description_json: form.value.description_json ?? null,
			}),
		);
		descriptionEditFor.value = form.value.id ?? null;
	};
	const blockMdEditorRef = ref<any>(null);
	const markdownEditorRef = ref<any>(null);
	const finishDescriptionEdit = async () => {
		blockMdEditorRef.value?.flush?.();
		await blockEditorRef.value?.flush?.();
		descriptionEditFor.value = null;
	};
	const cancelDescriptionEdit = async () => {
		// Unmount first: a late editor emit must not overwrite the revert.
		descriptionEditFor.value = null;
		await nextTick();
		for (const field of ['description', 'description_json'] as const) {
			const value = descriptionSnapshot[field] ?? null;
			if (JSON.stringify(form.value[field] ?? null) !== JSON.stringify(value))
				onDescriptionInput(field, value);
		}
	};
	// Window capture runs before the modal's document-level Esc handler and
	// alongside Crepe's own window listener, so an open editor popup closes
	// instead of the edit (or the task modal).
	const onDescriptionEsc = (e: KeyboardEvent) => {
		if (
			e.key !== 'Escape' ||
			e.isComposing ||
			!descriptionWrapperRef.value?.contains(e.target as Node)
		)
			return;
		e.stopPropagation();
		if (
			document.querySelector(
				'.ce-popover--opened, .milkdown-slash-menu[data-show="true"], .cm-search',
			)
		)
			return;
		e.preventDefault();
		void cancelDescriptionEdit();
	};
	watch(isDescriptionEditOpen, (open) => {
		if (open) window.addEventListener('keydown', onDescriptionEsc, true);
		else window.removeEventListener('keydown', onDescriptionEsc, true);
	});
	onBeforeUnmount(() =>
		window.removeEventListener('keydown', onDescriptionEsc, true),
	);
	const permissionDenied = ref(false);
	const taskReady = ref(!taskId.value);
	const taskLoadError = ref(false);
	const titleTextarea = ref<HTMLTextAreaElement | null>(null);
	const taskCommentsRef = ref<InstanceType<typeof TaskComments> | null>(null);
	const commentAttachments = ref<InstanceType<
		typeof CommentAttachments
	> | null>(null);
	const attachmentsBlocking = computed(
		() => commentAttachments.value?.canSend === false,
	);
	const taskRelationsRef = ref<InstanceType<typeof TaskRelations> | null>(null);
	const lastTaskSettings = ref(
		taskId.value
			? null
			: loadLastTaskSettings(store.getters.currentWorkspace?.code ?? ''),
	);
	const showLastTaskSettingsPrompt = computed(
		() =>
			!taskId.value &&
			!form.value.id &&
			!!statuses.value &&
			hasTaskSettings(lastTaskSettings.value),
	);
	const rememberTaskSettings = (created: Task) => {
		saveLastTaskSettings(
			store.getters.workspaceById(created.workspace_id)?.code ??
				store.getters.currentWorkspace?.code ??
				'',
			{
				status_id: form.value.status_id || null,
				project_category_id: form.value.project_category_id || null,
				assignees: [...assignees.value],
				persona_assignees: personaAssignees.value.map((p) => ({ ...p })),
				approximately_time: Number(form.value.approximately_time) || 0,
			},
		);
	};
	const applyLastTaskSettings = () => {
		const saved = lastTaskSettings.value;
		lastTaskSettings.value = null;
		if (!saved) return;
		const settings = applicableTaskSettings(saved, {
			statusIds: (statuses.value || workspaceStatuses.value || []).map(
				(status) => status.id,
			),
			categoryIds: categories.value.map((category) => category.id),
			memberIds: workspaceMembers.value.map((member) => member.id),
		});
		if (settings.status_id) form.value.status_id = settings.status_id;
		if (
			settings.project_category_id &&
			settings.project_category_id !== form.value.project_category_id
		) {
			form.value.project_category_id = settings.project_category_id;
			taskKeyNumber.value = '';
			if (!form.value.title) void updateTaskTitle();
		}
		if (settings.assignees.length) assignees.value = [...settings.assignees];
		if (isFeatureEnabled('personas')) {
			const personas = settings.persona_assignees.filter((persona) =>
				assignablePersonas.value.some((p) => p.id === persona.id),
			);
			if (personas.length) personaAssignees.value = personas;
		}
		if (settings.approximately_time > 0)
			form.value.approximately_time = settings.approximately_time;
	};

	function openLinkDialog() {
		taskRelationsRef.value?.startAdding?.();
	}
	const newComment = ref('');
	const isSendingComment = ref(false);
	const isCommentInputExpanded = ref(false);
	// TM-229: the right-rail composer lines up with the form footer, whose height depends on its
	// content, so it is measured rather than guessed at in padding.
	const footer = ref<HTMLElement | null>(null);
	const footerHeight = ref(0);
	let footerResizeObserver: ResizeObserver | null = null;
	const commentTextarea = ref<HTMLTextAreaElement | null>(null);
	const commentRailInput = ref<HTMLTextAreaElement | null>(null);
	const formMain = ref<HTMLElement | null>(null);
	const commentsRailScroller = ref<HTMLElement | null>(null);

	const resizeCommentInputs = () => {
		const pinned = [
			props.isModal ? formMain.value : null,
			commentsRailScroller.value,
		].filter(
			(el): el is HTMLElement =>
				!!el && el.scrollHeight - el.scrollTop - el.clientHeight <= 4,
		);
		for (const el of [commentTextarea.value, commentRailInput.value]) {
			if (!el || !el.scrollHeight) continue;
			el.style.height = 'auto';
			el.style.height = `${el.scrollHeight}px`;
		}
		for (const el of pinned) el.scrollTop = el.scrollHeight;
	};
	watch([newComment, commentTextarea, commentRailInput], resizeCommentInputs, {
		flush: 'post',
	});
	onMounted(() => window.addEventListener('resize', resizeCommentInputs));
	onBeforeUnmount(() =>
		window.removeEventListener('resize', resizeCommentInputs),
	);
	const commentsCount = ref(0);
	const showGitActivityModal = ref(false);
	const gitActivityCount = ref(0);
	const showCursorAgentModal = ref(false);
	const cursorAgents = ref<CursorAgent[]>([]);
	const categoryHasRepository = ref(false);
	const categoryCursorConfigured = ref(false);
	const categoryGitHubLoaded = ref(false);
	const categoryCursorLoaded = ref(false);
	const integrationHint = ref<CategoryIntegrationHint | null>(null);
	const pomodoroBlockRef = ref<any>(null);
	const pomodoroEnabled = computed(() => !!pomodoroBlockRef.value?.state);
	const pomodoroBusy = computed(
		() =>
			!!pomodoroBlockRef.value?.enabling || !!pomodoroBlockRef.value?.loading,
	);
	const mainTimerRunning = computed(
		() => !!form.value.start_time && form.value.start_time > 0,
	);
	const togglePomodoro = async () => {
		const inst = pomodoroBlockRef.value;
		if (!inst) return;
		if (inst.state) {
			await inst.handleDisable();
		} else {
			await inst.handleEnable();
		}
	};
	const handlePomodoroRequestMainStart = async () => {
		if (mainTimerRunning.value) return;
		await toggleTimer();
	};

	const currentCategoryCode = computed(() => {
		if (!form.value.project_category_id || !categories.value.length)
			return null;
		const category = categories.value.find(
			(c: Category) => c.id === form.value.project_category_id,
		);
		return category?.code || null;
	});

	const taskKeyError = ref<string | null>(null);

	const taskDisplayKey = computed(() => {
		if (
			currentCategoryCode.value &&
			form.value.category_tasks_sequence_id != null
		) {
			return `${currentCategoryCode.value}-${form.value.category_tasks_sequence_id}`;
		}
		return `TMGR-T${taskId.value || form.value.id}`;
	});

	/** TM-232: keep creating in the same category - opens the new-task form with it preselected. */
	const createAnotherInCategory = () => {
		store.commit('createTaskInProjectCategoryId', {
			projectCategoryId: form.value.project_category_id ?? null,
			statusId: null,
		});
	};

	const taskKeyNumber = computed({
		get: () => form.value.category_tasks_sequence_id ?? '',
		set: (value: number | string) => {
			if (value === '') {
				form.value.category_tasks_sequence_id = null;
				taskKeyError.value = null;
				return;
			}
			const parsed = Number(value);
			if (!Number.isInteger(parsed) || parsed < 1) {
				taskKeyError.value =
					'Enter a whole number of 1 or greater, or leave it blank to auto-assign.';
				return;
			}
			form.value.category_tasks_sequence_id = parsed;
			taskKeyError.value = null;
		},
	});

	const handleTaskSaveError = (e: any) => {
		console.error(e);
		if (e?.response?.status === 409) {
			taskKeyError.value =
				"There's a conflict saving this task — if you set a custom key, try another number or clear it to auto-assign.";
		}
	};

	const hasActiveAgent = computed(() => {
		return (
			isFeatureEnabled('cursor') &&
			cursorAgents.value.some((a: any) => a.status === 'RUNNING')
		);
	});

	const canRunWithCursor = computed(() => {
		return (
			isFeatureEnabled('cursor') && form.value.id && currentCategoryCode.value
		);
	});

	const autoResizeTitle = () => {
		if (titleTextarea.value) {
			titleTextarea.value.style.height = 'auto';
			titleTextarea.value.style.height =
				titleTextarea.value.scrollHeight + 'px';
		}
	};

	const isTitleNearLimit = computed(
		() => (form.value.title?.length || 0) >= 230,
	);
	const isTitleAtLimit = computed(() => (form.value.title?.length || 0) >= 255);

	// Keydown is the typing hook because WebKit hands beforeinput an empty `data`
	// once maxlength is reached; beforeinput still covers virtual keyboards.
	let overflowPending = '';
	let overflowFlushing = false;
	const carryToDescription = async (text: string) => {
		// While the editor mounts, focus stays in the title: later characters queue up.
		if (overflowFlushing) {
			overflowPending += text;
			return;
		}
		const carried = text.trimStart();
		if (!carried) return;
		overflowFlushing = true;
		const formId = form.value.id;
		overflowPending = carried;
		let taken = '';
		let inserted = false;
		try {
			if (!showDescriptionEditor.value) startDescriptionEdit();
			const current = () =>
				editorType.value === 'markdown'
					? markdownEditorRef.value
					: editorType.value === 'block'
						? blockEditorRef.value
						: blockMdEditorRef.value;
			for (let i = 0; !current()?.prependParagraph && i < 300; i++)
				await new Promise((resolve) => setTimeout(resolve, 16));
			inserted =
				(await current()?.prependParagraph?.(() => {
					taken = overflowPending;
					overflowPending = '';
					return taken;
				})) === true;
		} catch (error) {
			console.error(
				'Carrying the title overflow into the description failed',
				error,
			);
		} finally {
			let lost = inserted ? '' : taken;
			if (inserted && overflowPending) {
				// Typed while focus was moving: the editor has it now, continue there.
				if (
					!descriptionWrapperRef.value?.contains(document.activeElement) ||
					!document.execCommand('insertText', false, overflowPending)
				)
					lost = overflowPending;
			} else lost += overflowPending;
			overflowFlushing = false;
			overflowPending = '';
			if (
				lost &&
				form.value.id === formId &&
				(!formId || descriptionEditFor.value === formId)
			)
				writeCarriedToModel(lost);
		}
	};
	const writeCarriedToModel = (text: string) => {
		if (editorType.value === 'block')
			onDescriptionInput(
				'description_json',
				prependCarriedBlock(form.value.description_json, text),
			);
		else
			onDescriptionInput(
				'description',
				joinCarriedMarkdown(text, form.value.description ?? ''),
			);
	};
	let titleInsertBusy = false;
	const overflowTitleInsert = (
		event: Event,
		el: HTMLTextAreaElement,
		text: string,
		pasted: boolean,
	) => {
		const { selectionStart: start, selectionEnd: end, value } = el;
		const { insert, overflow } = splitTitleInsert(
			value.slice(0, start),
			value.slice(end),
			text,
		);
		if (!overflow || (!pasted && !(start === end && end === value.length)))
			return;
		event.preventDefault();
		if (insert) {
			titleInsertBusy = true;
			try {
				if (!document.execCommand('insertText', false, insert)) {
					el.setRangeText(insert, start, end, 'end');
					el.dispatchEvent(new Event('input', { bubbles: true }));
				}
			} finally {
				titleInsertBusy = false;
			}
		}
		void carryToDescription(overflow);
	};
	// Option types real characters on macOS; Alt is an accelerator elsewhere.
	const isMacPlatform = /mac|iphone|ipad/i.test(navigator.platform);
	const onTitleKeydown = (e: KeyboardEvent) => {
		if (
			e.key.length !== 1 ||
			e.ctrlKey ||
			e.metaKey ||
			(e.altKey && !isMacPlatform) ||
			e.isComposing ||
			e.keyCode === 229
		)
			return;
		overflowTitleInsert(
			e,
			e.currentTarget as HTMLTextAreaElement,
			e.key,
			false,
		);
	};
	const onTitleBeforeInput = (e: Event) => {
		const { inputType, data, isComposing } = e as InputEvent;
		if (titleInsertBusy || inputType !== 'insertText' || isComposing || !data)
			return;
		overflowTitleInsert(e, e.currentTarget as HTMLTextAreaElement, data, false);
	};
	const onTitlePaste = (e: ClipboardEvent) => {
		const text = e.clipboardData?.getData('text/plain');
		if (text)
			overflowTitleInsert(
				e,
				e.currentTarget as HTMLTextAreaElement,
				text,
				true,
			);
	};

	const checkpointsTotalLabel = computed(() => {
		const items = form.value.checkpoints || [];
		const total = items.reduce(
			(sum: number, c: any) =>
				sum + Math.max(0, (c?.end || 0) - (c?.start || 0)),
			0,
		);
		if (total <= 0) return '';
		const hours = Math.floor(total / 3600);
		const minutes = Math.floor((total % 3600) / 60);
		if (hours && minutes) return `${hours}h ${minutes}m`;
		if (hours) return `${hours}h`;
		return `${minutes}m`;
	});

	const simpleStatuses = computed<Status[]>(
		() => statuses.value || workspaceStatuses.value || [],
	);
	const defaultStatusIdFor = (list: Status[]) =>
		(simpleMode.value ? openTarget(list) : null) ??
		pickDefaultStatusId(list) ??
		list[0].id;
	const canMarkDone = computed(() => doneTarget(simpleStatuses.value) !== null);
	const doneChecked = computed({
		get: () => isTaskDone(form.value, simpleStatuses.value),
		set: (checked: boolean) => {
			const target = checked
				? doneTarget(simpleStatuses.value)
				: openTarget(simpleStatuses.value);
			if (target !== null) form.value.status_id = target;
		},
	});
	const isArchivedForm = computed(() =>
		isTaskArchived(form.value, simpleStatuses.value),
	);
	const canArchiveTask = computed(
		() =>
			simpleMode.value &&
			!!form.value.id &&
			!isArchivedForm.value &&
			archiveTarget(simpleStatuses.value) !== null,
	);

	const statusIdStr = computed({
		get: () => form.value.status_id?.toString() || '',
		set: (value: string) => {
			form.value.status_id = parseInt(value, 10);
		},
	});

	const priorityValue = computed({
		get: () => normalizePriority(form.value.priority),
		set: (value: string) => {
			form.value.priority = normalizePriority(value);
		},
	});

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
		tourBus.emit('checkpoint.added', {
			taskId: Number(formValue.id ?? taskId.value) || null,
		});
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
				(setting) => setting.key === 'current_workspace',
			)?.value;

			// Get preferred editor from settings for future use
			// If settings value is different from localStorage, we'll update localStorage
			const normalizedServerEditor = normalizeEditorType(
				store.state.user?.settings?.find(
					(setting) => setting.key === 'preferred_editor',
				)?.value,
			);

			// Only update if it's a valid type and different from current
			if (
				normalizedServerEditor &&
				normalizedServerEditor !== editorType.value
			) {
				console.log(
					'Editor preference in store differs from localStorage, updating to:',
					normalizedServerEditor,
				);
				setEditorType(normalizedServerEditor);
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

			const loadReferenceData = (taskWorkspaceId: number | null) =>
				loadTaskFormReferenceData(
					{
						taskWorkspaceId,
						currentWorkspaceId: workspaceId ? Number(workspaceId) : null,
					},
					{
						getStatuses,
						getStatusesOfWorkspace,
						getCategories,
						getCategoriesOfWorkspace,
						getWorkspaceMembers,
						getAssignablePersonas: (id: number) =>
							isFeatureEnabled('personas')
								? getAssignablePersonas(id)
								: Promise.resolve([]),
					},
				)
					.then(
						({
							statuses: loadedStatuses,
							categories: loadedCategories,
							members: loadedWorkspaceMembers,
							personas: loadedAssignablePersonas,
						}) => {
							if (formDisposed) return;
							assignablePersonas.value = loadedAssignablePersonas;

							statuses.value = loadedStatuses;
							categories.value = loadedCategories;
							workspaceMembers.value = loadedWorkspaceMembers;

							// Default new tasks to the Backlog (type 'default') status, falling back
							// to an active one — never Archived, which the API can return first.
							if (
								!taskId.value &&
								!form.value.status_id &&
								statuses.value?.length > 0
							) {
								form.value.status_id = defaultStatusIdFor(statuses.value);
							}

							// Title defaults only apply to a new task. Metadata must not hold an existing task behind it.
							if (!taskId.value) void updateTaskTitle();
						},
					)
					.catch(() => {
						if (!formDisposed)
							saveError.value =
								'Some task options could not load. Reload to retry.';
					});

			loadTaskReferenceData = loadReferenceData;
			if (!taskId.value) void loadReferenceData(null);

			// Load task data if we have a task ID
			if (taskId.value) {
				if (props.isModal && !props.detached) {
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
				const openingTaskId = taskId.value;
				const taskData = await getTask(openingTaskId);
				if (formDisposed || taskId.value !== openingTaskId) return;

				// Ensure approximately_time is a number
				taskData.approximately_time =
					typeof taskData.approximately_time === 'string'
						? parseInt(taskData.approximately_time, 10) || 0
						: taskData.approximately_time || 0;

				applyResolvedEditor(taskData);

				form.value = taskData;
				taskVersionGuard.recordKnown(taskData);
				void loadReferenceData(
					taskData.workspace_id ? Number(taskData.workspace_id) : null,
				);

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

				// Handle assignees
				if (form.value.assignees) {
					const taskAssignees = form.value.assignees as WorkspaceMember[];
					assignees.value = taskAssignees.map((assignee) => assignee.id);
				} else {
					assignees.value = [];
				}
				personaAssignees.value = personaAssigneesOf(form.value);

				taskReady.value = true;
				void Promise.allSettled([
					loadGitActivity(),
					loadCursorAgents(),
					loadCategoryIntegrationState(),
				]);

				nextTick(() => {
					autoResizeTitle();
					titleTextarea.value?.focus();
				});
			}
		} catch (e: any) {
			console.error('Error loading task data:', e);
			taskLoadError.value = true;
			if (e.response?.status === 403) {
				permissionDenied.value = true;
			}
		}
	});

	onMounted(() => {
		if (footer.value && typeof ResizeObserver !== 'undefined') {
			footerResizeObserver = new ResizeObserver(() => {
				footerHeight.value = footer.value?.offsetHeight ?? 0;
			});
			footerResizeObserver.observe(footer.value);
			footerHeight.value = footer.value.offsetHeight;
		}

		// Only focus on title for new tasks
		// Pattern is already applied in onBeforeMount, so we just need to focus
		if (!taskId.value && props.isModal) {
			// Wait for modal transition and DOM to be fully ready
			setTimeout(() => {
				nextTick(() => {
					titleTextarea.value?.focus();
				});
			}, 350);
		}
	});

	const currentWorkspaceIdFromSettings = computed<number | null>(() => {
		const setting = store.state.user?.settings?.find(
			(item: any) => item.key === 'current_workspace',
		);
		return setting ? +setting.value : null;
	});

	const subscribeToWorkspaceEvents = (workspaceId: number) => {
		if (subscribedWorkspaceId.value && pusherSubscriptionId.value) {
			unsubscribeHandlerFromWorkspace(
				subscribedWorkspaceId.value,
				pusherSubscriptionId.value,
			);
		}
		subscribedWorkspaceId.value = workspaceId;
		pusherSubscriptionId.value = subscribeToWorkspace(workspaceId, {
			onTaskUpdated: (task, action, _updatedByUserId, sourceInstanceId) => {
				console.log('[NewForm] onTaskUpdated received:', {
					taskId: task.id,
					myTaskId: taskId.value,
					action,
					sourceInstanceId,
					myInstanceId: instanceId,
				});
				if (task.id !== taskId.value) return;
				if (sourceInstanceId === instanceId) return;

				if (action === 'updated') {
					syncRemoteRelations(task);
					taskVersionGuard.receive(task);
				} else if (action === 'deleted') {
					emit('close');
				}
			},
			onTaskAssignmentChanged: ({ task_id }) => {
				void refreshAssignments(task_id);
			},
			onCommentAdded: (comment) => {
				if (comment.task_id === taskId.value) {
					taskCommentsRef.value?.loadComments?.();
				}
			},
			onCommentUpdated: (comment) => {
				if (comment.task_id === taskId.value) {
					taskCommentsRef.value?.loadComments?.();
				}
			},
			onCommentDeleted: (comment) => {
				if (comment.task_id === taskId.value) {
					taskCommentsRef.value?.loadComments?.();
				}
			},
			onCommentReactionsUpdated: (e) => {
				if (form.value.id && e.task_id === form.value.id) {
					taskCommentsRef.value?.applyReactions?.(e);
				}
			},
		});
	};

	// On a hard reload of /:ws/tasks/:id the router guard does not await getUser(),
	// so current_workspace usually lands after this component mounted. Subscribe
	// whenever it becomes known instead of only in onMounted.
	watch(
		currentWorkspaceIdFromSettings,
		(workspaceId) => {
			if (workspaceId) {
				subscribeToWorkspaceEvents(workspaceId);
			}
		},
		{ immediate: true },
	);

	const resyncTimer = async () => {
		const id = form.value.id;
		if (!id) return;
		try {
			applyTimerState(form.value, (await getTask(id, false)) as any);
		} catch (error) {
			console.error('Failed to resync task timer:', error);
		}
	};

	useRunningTimerSync({
		onEvent: (task) => {
			if (form.value.id && Number(task?.id) === Number(form.value.id)) {
				requestCache.invalidate(`task-${form.value.id}`);
			}
			applyTimerState(form.value, task);
		},
		onResync: resyncTimer,
	});

	watch(
		() => form.value.id,
		() => {
			aiPending.value = false;
			aiPendingSteps.value = [];
		},
	);

	watch(
		() => store.state.user?.id,
		(userId) => {
			if (subscribedUserId.value && userPusherSubscriptionId.value) {
				unsubscribeHandler(
					`App.User.${subscribedUserId.value}`,
					userPusherSubscriptionId.value,
				);
			}
			if (!userId) return;
			subscribedUserId.value = userId;
			userPusherSubscriptionId.value = subscribeToUser(userId, {
				onAgentStep: (e) => {
					if (
						isForThisTask(e) &&
						!aiPendingSteps.value.some((s) => s.seq === e.seq)
					) {
						aiPendingSteps.value = [
							...aiPendingSteps.value,
							{ seq: e.seq, tool: e.tool, summary: e.summary },
						];
					}
				},
				onAgentReply: (e) => {
					if (isForThisTask(e)) {
						aiPending.value = false;
						aiPendingSteps.value = [];
						taskCommentsRef.value?.loadComments?.();
					}
				},
			});
		},
		{ immediate: true },
	);

	onUnmounted(() => {
		formDisposed = true;
		footerResizeObserver?.disconnect();
		footerResizeObserver = null;
		unregisterModal(checkpointsModalId);
		store.commit('removeModalFromStack', checkpointsModalId);
		if (subscribedWorkspaceId.value && pusherSubscriptionId.value) {
			unsubscribeHandlerFromWorkspace(
				subscribedWorkspaceId.value,
				pusherSubscriptionId.value,
			);
		}
		if (subscribedUserId.value && userPusherSubscriptionId.value) {
			unsubscribeHandler(
				`App.User.${subscribedUserId.value}`,
				userPusherSubscriptionId.value,
			);
		}
	});

	const applyExternalUpdate = () => {
		if (externalUpdateData.value) {
			taskVersionGuard.recordKnown(externalUpdateData.value);
			applyResolvedEditor({ ...form.value, ...externalUpdateData.value });
			Object.assign(form.value, externalUpdateData.value);
			hasExternalUpdate.value = false;
			externalUpdateData.value = null;
		}
	};

	const dismissExternalUpdate = () => {
		hasExternalUpdate.value = false;
		externalUpdateData.value = null;
	};

	const reloadTask = async () => {
		if (!taskId.value) return;
		try {
			suppressAutoSavingForOnce.value = true;
			const id = taskId.value;
			const taskData = await getTask(id);
			if (formDisposed || taskId.value !== id) return;
			applyResolvedEditor(taskData);
			form.value = taskData;
			taskVersionGuard.recordKnown(taskData);
			if (!statuses.value?.length)
				void loadTaskReferenceData?.(
					taskData.workspace_id ? Number(taskData.workspace_id) : null,
				);
			taskReady.value = true;
			taskLoadError.value = false;
		} catch (e: any) {
			console.error('Error reloading task:', e);
		}
	};

	const handleOpenLinkedTask = (linkedTaskId: number) => {
		if (props.detached) {
			const target = taskWindowTarget(
				{ id: linkedTaskId },
				store.state.workspaces,
				store.getters.currentWorkspace,
			);
			if (target) void openTaskWindow(target);
		} else if (props.isModal) {
			store.state.currentTaskIdForModal = linkedTaskId;
		} else {
			router.push(
				`/${
					store.getters['user/getCurrentWorkspace']?.code || 'default'
				}/tasks/${linkedTaskId}`,
			);
		}
	};

	const loadGitActivity = async () => {
		if (!form.value.id || !isFeatureEnabled('github')) {
			console.log('[Git Activity] No task ID, skipping');
			return;
		}
		try {
			console.log('[Git Activity] Loading for task:', form.value.id);
			const id = form.value.id;
			const activity = await getTaskGitActivity(id);
			if (formDisposed || form.value.id !== id) return;
			const count =
				(activity.commits?.length || 0) +
				(activity.branches?.length || 0) +
				(activity.pull_requests?.length || 0);
			gitActivityCount.value = count;
			console.log('[Git Activity] Loaded count:', count, activity);
		} catch (e) {
			console.error('[Git Activity] Failed to load:', e);
			gitActivityCount.value = 0;
		}
	};

	const loadCursorAgents = async () => {
		if (!form.value.id || !isFeatureEnabled('cursor')) return;
		try {
			const id = form.value.id;
			const result = await getCursorAgents(id);
			if (formDisposed || form.value.id !== id) return;
			cursorAgents.value = result;
		} catch (e) {
			console.error('[Cursor] Failed to load agents:', e);
			cursorAgents.value = [];
		}
	};

	const loadCategoryIntegrationState = async () => {
		const categoryId = form.value.project_category_id;
		categoryGitHubLoaded.value = false;
		categoryCursorLoaded.value = false;
		if (!categoryId) return;
		const moduleOff = () => Promise.reject(new Error('module is off'));
		const [github, cursor] = await Promise.allSettled([
			isFeatureEnabled('github')
				? getCategoryGitHubStatus(categoryId)
				: moduleOff(),
			isFeatureEnabled('cursor') ? getCursorStatus(categoryId) : moduleOff(),
		]);
		// Ignore stale responses if the category changed while this load was in flight.
		if (form.value.project_category_id !== categoryId) return;
		if (github.status === 'fulfilled') {
			categoryHasRepository.value = !!github.value.repository;
			categoryGitHubLoaded.value = true;
		}
		if (cursor.status === 'fulfilled') {
			categoryCursorConfigured.value = !!cursor.value.configured;
			categoryCursorLoaded.value = true;
		}
	};

	// Reload integration status when the category changes so the hint reflects the
	// currently selected category, not the one loaded at open time.
	watch(
		() => form.value.project_category_id,
		() => {
			loadCategoryIntegrationState();
		},
	);

	const integrationState = () => ({
		hasRepository: categoryHasRepository.value,
		cursorConfigured: categoryCursorConfigured.value,
	});

	const openCursorAgent = () => {
		const hint = getCategoryIntegrationHint('cursor', integrationState());
		if (categoryCursorLoaded.value && hint.show) {
			integrationHint.value = hint;
			return;
		}
		showCursorAgentModal.value = true;
	};

	const openGitActivity = () => {
		const hint = getCategoryIntegrationHint('github', integrationState());
		if (categoryGitHubLoaded.value && hint.show) {
			integrationHint.value = hint;
			return;
		}
		showGitActivityModal.value = true;
	};

	const goToCategorySettings = () => {
		const categoryId = form.value.project_category_id;
		const workspaceCode = store.getters.currentWorkspace?.code;
		integrationHint.value = null;
		if (!categoryId || !workspaceCode) return;
		if (props.isModal && !props.detached) {
			store.commit('closeTaskModal');
		}
		router.push({
			name: 'WorkspaceCategoryEdit',
			params: { workspace_code: workspaceCode, id: categoryId },
		});
	};

	const sendToCursor = async () => {
		const activeAgent = cursorAgents.value.find(
			(a: any) => a.status === 'RUNNING',
		);
		if (!activeAgent || !form.value.id || !newComment.value.trim()) return;

		try {
			await sendFollowUp(form.value.id, activeAgent.id, newComment.value);
			newComment.value = '';
			focusCommentInput();
			if (taskCommentsRef.value) {
				taskCommentsRef.value.loadComments();
			}
		} catch (e) {
			console.error('[Cursor] Failed to send follow-up:', e);
		}
	};

	const updateTaskTitle = async () => {
		if (categories.value.length > 0 && form.value.project_category_id) {
			const currentCategory = categories.value.find(
				(category) => category.id === form.value.project_category_id,
			);
			const taskNamePatternSetting = currentCategory?.settings?.find(
				(setting) => setting.key === 'task_name_pattern_date&time',
			);
			console.log(
				categories.value,
				form.value.project_category_id,
				taskNamePatternSetting,
				currentCategory,
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

	const isLocalWorkspaceTask = computed(
		() => Number(store.getters.currentWorkspaceId) < 0,
	);

	const creator = computed(() =>
		taskCreator(
			workspaceMembers.value,
			form.value.user_id,
			isLocalWorkspaceTask.value,
		),
	);

	const exportTaskToMarkdown = async () => {
		try {
			const { exportLocalTask } = await import('@/local/runtime');
			await exportLocalTask(
				Number(taskId.value || form.value.id),
				store.state.user?.name ?? '',
			);
		} catch (error) {
			console.error('Markdown export failed', error);
		}
	};

	const isCreatingTask = ref(false);
	const pendingFiles = ref<File[]>([]);
	const attachmentsRef = ref<InstanceType<typeof TaskAttachments> | null>(null);
	const { toast } = useToast();
	const { items: pluginTaskMenuItems } = usePluginTaskMenu();

	const isUploadingPendingFiles = ref(false);

	const attachPendingFiles = async (newTaskId: number) => {
		const failed: File[] = [];
		isUploadingPendingFiles.value = true;
		try {
			while (pendingFiles.value.length) {
				const batch = pendingFiles.value;
				const result = await uploadPendingFiles(batch, (file) =>
					uploadTaskFile(newTaskId, file),
				);
				pendingFiles.value = pendingFiles.value.filter(
					(file) => !batch.includes(file),
				);
				failed.push(...result.failed);
			}
		} finally {
			isUploadingPendingFiles.value = false;
		}
		if (failed.length) {
			toast({
				title: 'Some files were not attached',
				description: failed.map((file) => file.name).join(', '),
				variant: 'destructive',
			});
		}
	};

	const fileDragDepth = createFileDragDepth();
	const isFileDragActive = ref(false);
	const acceptsFileDrag = (event: DragEvent) =>
		isFeatureEnabled('task.files') && isFileDrag(event);
	const resetFileDrag = () => {
		fileDragDepth.reset();
		isFileDragActive.value = false;
	};
	const onFormDragEnter = (event: DragEvent) => {
		if (!acceptsFileDrag(event)) {
			return;
		}
		event.preventDefault();
		isFileDragActive.value = fileDragDepth.enter();
	};
	const onFormDragOver = (event: DragEvent) => {
		if (acceptsFileDrag(event)) {
			event.preventDefault();
		}
	};
	const onFormDragLeave = (event: DragEvent) => {
		if (!acceptsFileDrag(event)) {
			return;
		}
		isFileDragActive.value = fileDragDepth.leave();
	};
	const onFormDrop = (event: DragEvent) => {
		if (!acceptsFileDrag(event)) {
			return;
		}
		resetFileDrag();
		event.preventDefault();
		event.stopPropagation();
		attachmentsRef.value?.uploadAll(
			Array.from(event.dataTransfer?.files ?? []),
		);
	};
	onMounted(() => {
		window.addEventListener('dragend', resetFileDrag);
		window.addEventListener('drop', resetFileDrag);
	});
	onBeforeUnmount(() => {
		window.removeEventListener('dragend', resetFileDrag);
		window.removeEventListener('drop', resetFileDrag);
	});

	const createTask = async () => {
		if (isCreatingTask.value) {
			return;
		}
		isCreatingTask.value = true;

		try {
			blockMdEditorRef.value?.flush?.();
			await blockEditorRef.value?.flush?.();
			updateFormBeforeQuery();
			suppressAutoSavingForOnce.value = true;
			const submittedText = {
				title: form.value.title,
				description: form.value.description,
			};
			const created = await createTaskAction(form.value as Task);
			if (created.id) {
				autoSignQuietly(created, submittedText);
				rememberTaskSettings(created);
				await attachPendingFiles(created.id as number);
			}
			if (formDisposed) {
				store.commit('taskCreated', created);
				return;
			}
			form.value = created;

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

			store.commit('taskCreated', form.value);
		} catch (e) {
			handleTaskSaveError(e);
		} finally {
			isCreatingTask.value = false;
		}
	};

	const suppressAutoSavingForOnce = ref(false);

	const updateFormBeforeQuery = () => {
		suppressAutoSavingForOnce.value = true;
		// Ensure assignees is properly typed as Record<string, any>[] | number[]
		form.value.assignees = assignees.value;
		if (!taskId.value && !form.value.id) {
			form.value.persona_assignees = personaAssignees.value.map((p) => p.id);
		}

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
			// Clear the JSON description since we're using markdown editor
			form.value.description_json = null;
		}
	};

	let formDisposed = false;
	const blockEditorRef = ref<any>(null);
	const draftOwner = store.state.user?.id;
	const draftKey = (task: Task) =>
		`tmgr:task-draft:${draftOwner}:${task.workspace_id}:${task.id}`;
	const saveError = ref('');
	const recoverableDraft = ref<Task | null>(null);
	const loadDraft = () => {
		if (!form.value.id) return;
		try {
			recoverableDraft.value = JSON.parse(
				sessionStorage.getItem(draftKey(form.value as Task)) || 'null',
			);
		} catch {
			recoverableDraft.value = null;
		}
	};
	const restoreDraft = async () => {
		if (!recoverableDraft.value || recoverableDraft.value.id !== form.value.id)
			return;
		assignees.value = (recoverableDraft.value.assignees || []).map(
			(item: any) => (typeof item === 'object' ? item.id : item),
		);
		form.value = { ...form.value, ...recoverableDraft.value };
		applyResolvedEditor(form.value);
		const draft = recoverableDraft.value as Record<string, unknown>;
		descriptionDirty.value = !!(
			draft.clear_description || draft.clear_description_json
		);
		recoverableDraft.value = null;
		try {
			// Queue replacement before any forced read of the editor DOM.
			await blockEditorRef.value?.replace(form.value.description_json);
			await saveTask();
		} catch (error) {
			saveError.value =
				'Could not restore the editor. Your draft is still available.';
			loadDraft();
		}
	};
	const discardDraft = () => {
		sessionStorage.removeItem(draftKey(form.value as Task));
		recoverableDraft.value = null;
	};
	watch([() => form.value.id, () => form.value.workspace_id], loadDraft, {
		immediate: true,
	});
	const persistTaskSnapshot = async (snapshot: Task) => {
		if (!snapshot.id) return;
		const key = draftKey(snapshot);
		const serialized = JSON.stringify(snapshot);
		try {
			if (!sessionStorage.getItem(key)) sessionStorage.setItem(key, serialized);
		} catch {
			/* Saving still works if browser storage is unavailable. */
		}
		const body = toUpdateBody({
			...snapshot,
			...(snapshot.persona_assignees === undefined
				? {}
				: { persona_assignees: personaAssigneeIds(snapshot) }),
		}) as Task;
		const saved = await taskVersionGuard.track(() =>
			updateTask(snapshot.id as number, body, instanceId),
		);
		try {
			if (sessionStorage.getItem(key) === serialized)
				sessionStorage.removeItem(key);
		} catch {
			/* Storage unavailable. */
		}
		store.commit('updateSingleTask', saved);
		autoSignQuietly(saved, {
			title: body.title,
			description: body.description,
		});
		if (!formDisposed && form.value.id === snapshot.id) {
			saveError.value = '';
			suppressAutoSavingForOnce.value = true;
			form.value = mergeSavedTask(form.value as Task, snapshot, saved);
		}
	};
	const saveTask = async () => {
		if (!form.value.id) return;
		const id = form.value.id;
		await blockEditorRef.value?.flush();
		if (formDisposed || form.value.id !== id) return;
		await flushAutoSave(true);
	};

	const deleteCurrentTask = async () => {
		if (!taskId.value && !form.value.id) return;

		isLoading.value = true;

		try {
			const id = taskId.value || (form.value.id as number);
			if (form.value.start_time) {
				suppressAutoSavingForOnce.value = true;
				form.value = await taskVersionGuard.track(() =>
					stopTaskTimeCounter(id),
				);
			}

			cancelPendingAutoSave();
			await deleteTask(id);

			if (props.isModal) {
				emit('close');
				store.commit('taskDeleted', id);
			} else {
				router.push('/app');
			}
		} catch (e) {
			console.error(e);
		} finally {
			isLoading.value = false;
		}
	};

	const backlogStatusChangeConfirm = ref<{
		taskId: number;
		prompt: BacklogTimerPrompt;
	} | null>(null);

	const toggleTimer = async () => {
		if (!taskId.value && !form.value.id) return;

		const isTimerRunning = form.value.start_time && form.value.start_time > 0;

		if (isTimerRunning) {
			isLoading.value = true;
			try {
				suppressAutoSavingForOnce.value = true;
				const id = taskId.value || (form.value.id as number);
				form.value = await taskVersionGuard.track(() =>
					stopTaskTimeCounter(id),
				);
				store.commit('updateSingleTask', form.value);
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
			form.value = await taskVersionGuard.track(() => startTaskTimeCounter(id));
			store.commit('updateSingleTask', form.value);
		} catch (e) {
			console.error(e);
		} finally {
			isLoading.value = false;
		}

		const prompt = simpleMode.value
			? null
			: backlogTimerPrompt(
					form.value,
					statuses.value || workspaceStatuses.value || [],
				);
		if (prompt) {
			backlogStatusChangeConfirm.value = {
				taskId: taskId.value || (form.value.id as number),
				prompt,
			};
			showBacklogStatusChangeConfirm.value = true;
		}
	};

	const canUnarchiveTask = computed(
		() =>
			simpleMode.value &&
			!!form.value.id &&
			isArchivedForm.value &&
			openTarget(simpleStatuses.value) !== null,
	);

	const moveTaskToStatus = async (statusId: number) => {
		isLoading.value = true;
		try {
			form.value.status_id = statusId;
			await flushAutoSave(true);
			store.commit('updateSingleTask', form.value);
			store.commit('incrementReloadTasksKey');
			return true;
		} catch (e) {
			console.error('Failed to change task status:', e);
			return false;
		} finally {
			isLoading.value = false;
		}
	};

	const unarchiveTask = async () => {
		const openId = openTarget(simpleStatuses.value);
		if (openId === null || !(taskId.value || form.value.id)) return;
		await moveTaskToStatus(openId);
	};

	const archiveTask = async () => {
		const archiveId = archiveTarget(simpleStatuses.value);
		if (archiveId === null || !(taskId.value || form.value.id)) return;
		if ((await moveTaskToStatus(archiveId)) && props.isModal) emit('close');
	};

	const showBacklogStatusChangeConfirm = ref(false);

	const backlogConfirmBody = computed(
		() => backlogStatusChangeConfirm.value?.prompt.message ?? '',
	);

	const handleBacklogStatusChangeConfirm = async () => {
		if (!backlogStatusChangeConfirm.value) return;

		const activeStatus = backlogStatusChangeConfirm.value.prompt.targetStatus;

		isLoading.value = true;
		try {
			suppressAutoSavingForOnce.value = true;
			const id = backlogStatusChangeConfirm.value.taskId;
			await taskVersionGuard.track(() => updateTaskStatus(id, activeStatus.id));
			form.value.status_id = activeStatus.id;
			store.commit('updateSingleTask', form.value);
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
			'description_json',
			'project_category_id',
			'category_tasks_sequence_id',
			'assignees',
			'status',
			'status_id',
			'priority',
		],
		onSave: persistTaskSnapshot,
		enabled: () => taskReady.value && !!form.value.id,
		makeSnapshot: (value) => ({
			...value,
			assignees: [...assignees.value],
			description: editorType.value === 'block' ? null : value.description,
			description_json:
				editorType.value === 'block' ? value.description_json : null,
			clear_description: undefined,
			clear_description_json: undefined,
			...(descriptionDirty.value
				? buildDescriptionPayload(editorType.value, value)
				: {}),
		}),
		onDirty: (snapshot) => {
			try {
				sessionStorage.setItem(
					draftKey(snapshot as Task),
					JSON.stringify(snapshot),
				);
			} catch {
				/* Network save remains available. */
			}
		},
		onError: (error, snapshot) => {
			if (!formDisposed && form.value.id === snapshot.id) {
				saveError.value =
					'Could not save changes. Retry before closing this tab.';
				loadDraft();
				handleTaskSaveError(error);
			}
		},
		delay: 2000,
		suppressDebounceForOnce: suppressAutoSavingForOnce,
	});
	watch(
		assignees,
		(value) => {
			if (
				JSON.stringify(
					form.value.assignees?.map((item: any) =>
						typeof item === 'object' ? item.id : item,
					),
				) !== JSON.stringify(value)
			)
				form.value.assignees = [...value];
		},
		{ deep: true },
	);
	const isAutoSaving = autosaveResult[0];
	const cancelPendingAutoSave = autosaveResult[1];
	const flushAutoSave = autosaveResult[2];
	const enqueueTaskSnapshot = autosaveResult[3];
	const hasUnsavedChanges = autosaveResult[4];
	onBeforeUnmount(() => {
		const editor = blockEditorRef.value;
		if (form.value.id && editor) {
			const snapshot = JSON.parse(JSON.stringify(form.value)) as Task;
			snapshot.assignees = [...assignees.value] as any;
			void editor
				.flush()
				.then((content: any) => {
					if (
						content &&
						!sameBlocks(content.blocks, snapshot.description_json)
					) {
						snapshot.description_json = content;
						snapshot.description = null;
						Object.assign(snapshot, buildDescriptionPayload('block', snapshot));
						return enqueueTaskSnapshot(snapshot);
					}
				})
				.catch((error: unknown) =>
					console.error(
						'Could not serialize closing editor; existing draft retained',
						error,
					),
				);
		}
		formDisposed = true;
	});

	useMagicKeys({
		passive: false,
		onEventFired(e) {
			if (!isSaveHotkey(e)) {
				return;
			}
			e.preventDefault();
			if (!taskId.value && !form.value.id) {
				createTask();
			} else {
				saveTask();
			}
		},
	});

	const openInSeparateWindow = async () => {
		const target = taskWindowTarget(
			{ ...form.value, id: taskId.value || form.value.id },
			store.state.workspaces,
			store.getters.currentWorkspace,
		);
		if (!target) return;
		try {
			await openTaskWindow(target);
			emit('close');
		} catch (e) {
			console.error('Failed to open the task window:', e);
			toast({
				title: 'Could not open the task window',
				description: String(e),
				variant: 'destructive',
			});
		}
	};

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

	const applyAssignments = (data: Partial<Task> | null | undefined) => {
		if (!data?.assignees) return;
		suppressAutoSavingForOnce.value = true;
		form.value = {
			...form.value,
			assignees: data.assignees,
			persona_assignees: data.persona_assignees ?? [],
		};
		assignees.value = (data.assignees as Array<{ id: number } | number>).map(
			(a) => (typeof a === 'object' ? a.id : a),
		);
		personaAssignees.value = personaAssigneesOf(data);
		store.commit('updateSingleTask', form.value);
	};

	const togglePersona = async (persona: PersonaAssignee) => {
		const selected = personaAssignees.value.some((p) => p.id === persona.id);
		const id = taskId.value || form.value.id;
		if (!id) {
			personaAssignees.value = selected
				? personaAssignees.value.filter((p) => p.id !== persona.id)
				: [...personaAssignees.value, persona];
			return;
		}
		try {
			applyAssignments(
				selected
					? await unassignPersonaFromTask(id, persona.id)
					: await assignPersonaToTask(id, persona.id),
			);
		} catch (e) {
			console.error('Failed to update persona assignees:', e);
			toast({
				title: 'Could not update persona assignees',
				variant: 'destructive',
			});
		}
	};

	const refreshAssignments = async (changedTaskId: number) => {
		if (!form.value.id || Number(changedTaskId) !== form.value.id) return;
		try {
			applyAssignments(await getTask(form.value.id, false));
		} catch {
			/* The next full load picks the assignment up. */
		}
	};

	const isAssignedToMe = computed(() => {
		if (!currentUserId.value) return false;
		return assignees.value.includes(currentUserId.value);
	});

	const assignToMe = () => {
		if (!currentUserId.value || isAssignedToMe.value) return;
		assignees.value = [...assignees.value, currentUserId.value];
	};

	// Watch for changes to modalProjectCategoryId and update form accordingly
	watch(modalProjectCategoryId, async (newCategoryId) => {
		if (newCategoryId && !taskId.value) {
			form.value.project_category_id = newCategoryId;

			// Ensure a status is selected (prefer Backlog/active, never Archived)
			if (
				!form.value.status_id &&
				statuses.value &&
				statuses.value.length > 0
			) {
				form.value.status_id = defaultStatusIdFor(statuses.value);
			}

			await updateTaskTitle();
		}
	});

	// TM-222: the composer keeps the cursor - on expand, and again after a message is sent, so a
	// follow-up can be typed without clicking back into the field.
	const focusCommentInput = () =>
		nextTick(() => focusField(commentTextarea.value ?? commentRailInput.value));
	const blurCommentInput = (e: KeyboardEvent) => {
		e.stopPropagation();
		if (e.isComposing || e.keyCode === 229) return;
		(e.target as HTMLElement).blur();
	};

	watch(isCommentInputExpanded, (expanded) => {
		if (expanded) {
			focusCommentInput();
		}
	});

	watch(modalTaskId, async (newTaskId, oldTaskId) => {
		if (props.isModal && newTaskId && newTaskId !== oldTaskId) {
			await blockEditorRef.value?.flush();
			await flushAutoSave();
			taskReady.value = false;
			if (formDisposed || modalTaskId.value !== newTaskId) return;
			suppressAutoSavingForOnce.value = true;
			try {
				const taskData = await getTask(newTaskId);
				if (formDisposed || modalTaskId.value !== newTaskId) return;
				applyResolvedEditor(taskData);

				taskData.approximately_time =
					typeof taskData.approximately_time === 'string'
						? parseInt(taskData.approximately_time, 10) || 0
						: taskData.approximately_time || 0;

				form.value = taskData;

				if (!form.value.checkpoints) {
					form.value.checkpoints = [];
				}

				if (form.value.assignees) {
					const taskAssignees = form.value.assignees as WorkspaceMember[];
					assignees.value = taskAssignees.map((assignee) => assignee.id);
				} else {
					assignees.value = [];
				}
				personaAssignees.value = personaAssigneesOf(form.value);

				const currentWorkspace = store.getters['user/getCurrentWorkspace'];
				if (currentWorkspace && !props.detached) {
					const url = generateTaskUrl(newTaskId, currentWorkspace, null);
					if (url && url !== '/') {
						history.replaceState({}, '', url);
						store.state.urlManuallyChanged = true;
					}
				}

				nextTick(() => {
					autoResizeTitle();
					titleTextarea.value?.focus();
				});

				taskReady.value = true;
				void Promise.allSettled([
					loadGitActivity(),
					loadCursorAgents(),
					loadCategoryIntegrationState(),
				]);
			} catch (e: any) {
				console.error('Error loading linked task:', e);
			}
		}
	});

	watch(
		() => form.value.title,
		(newTitle: string) => {
			if (!props.isModal && newTitle) {
				setDocumentTitle(newTitle);
			}
			if (props.detached && newTitle?.trim()) {
				void setTaskWindowTitle(newTitle.trim());
			}
		},
		{ immediate: true },
	);

	const scrollMainToBottom = () => {
		nextTick(() => {
			const mainEl = document.querySelector(
				'.new-form-container main',
			) as HTMLElement | null;
			if (mainEl) {
				mainEl.scrollTo({ top: mainEl.scrollHeight, behavior: 'smooth' });
			}
			const rail = commentsRailScroller.value;
			rail?.scrollTo({ top: rail.scrollHeight, behavior: 'smooth' });
		});
	};

	const sendComment = async () => {
		if (
			!newComment.value.trim() ||
			isSendingComment.value ||
			attachmentsBlocking.value ||
			!form.value.id
		)
			return;

		isSendingComment.value = true;
		try {
			const fileIds = commentAttachments.value?.fileIds ?? [];
			await createComment(form.value.id, {
				message: newComment.value.trim(),
				...(fileIds.length ? { file_ids: fileIds } : {}),
			});
			newComment.value = '';
			commentAttachments.value?.clear();
			isCommentInputExpanded.value = false;
			focusCommentInput();
			if (taskCommentsRef.value) {
				await taskCommentsRef.value.loadComments();
			}
			scrollMainToBottom();
		} catch (error) {
			console.error('Failed to send comment:', error);
			const message = attachmentErrorMessage(error);
			if (message) commentAttachments.value?.setNotice(message);
		} finally {
			isSendingComment.value = false;
		}
	};

	const askAIComment = async () => {
		if (
			!newComment.value.trim() ||
			isSendingComment.value ||
			attachmentsBlocking.value ||
			!form.value.id
		)
			return;

		isSendingComment.value = true;
		try {
			await createAskingHelpComment(
				form.value.id,
				newComment.value.trim(),
				commentAttachments.value?.fileIds ?? [],
			);
			aiPending.value = true;
			aiPendingSteps.value = [];
			newComment.value = '';
			commentAttachments.value?.clear();
			isCommentInputExpanded.value = false;
			focusCommentInput();
			if (taskCommentsRef.value) {
				await taskCommentsRef.value.loadComments();
			}
			scrollMainToBottom();
		} catch (error) {
			console.error('Failed to ask AI:', error);
			const message = attachmentErrorMessage(error);
			if (message) commentAttachments.value?.setNotice(message);
			aiPending.value = false;
		} finally {
			isSendingComment.value = false;
		}
	};

	const commentAiReply = computed(
		() =>
			!isLocalWorkspaceTask.value &&
			isFeatureEnabled('ai.assistant') &&
			isCommentAiReplyOn(),
	);

	const toggleCommentAiReply = () => {
		setCommentAiReply(!isCommentAiReplyOn()).catch((error) =>
			console.error('Failed to save the AI reply setting:', error),
		);
	};

	const submitComment = () =>
		commentAiReply.value ? askAIComment() : sendComment();

	const onCommentEnter = (e: KeyboardEvent) => {
		if (!isSendEnter(e)) return;
		e.preventDefault();
		submitComment();
	};
</script>

<template>
	<div
		v-if="permissionDenied"
		class="flex min-h-[300px] items-center justify-center p-10"
	>
		<ForbiddenAccess :show-back-button="false" />
	</div>

	<div
		v-else-if="!taskReady"
		class="space-y-4 min-h-[500px] p-6"
		:class="{ 'mx-auto w-full max-w-7xl': !isModal }"
		:aria-busy="!taskLoadError"
	>
		<div v-if="taskLoadError" role="alert">
			Could not load task.
			<button class="underline" @click="reloadTask">Retry</button>
		</div>
		<TaskFormSkeleton v-else />
	</div>
	<div
		v-else
		class="new-form-container relative h-full font-display text-ink"
		:class="{ 'bg-surface-sunken': !isModal }"
		@dragenter="onFormDragEnter"
		@dragover="onFormDragOver"
		@dragleave="onFormDragLeave"
		@drop.capture="onFormDrop"
	>
		<div
			class="flex transition-all duration-300"
			:class="[
				isModal
					? 'h-full max-h-[100dvh] w-full flex-col overflow-hidden'
					: 'mx-auto w-full max-w-7xl flex-col border-line lg:h-[calc(100dvh-var(--app-chrome-h,4rem))] lg:flex-row lg:overflow-hidden min-[1537px]:border-x',
			]"
		>
			<!-- Form Panel (main / left column) -->
			<div
				class="flex w-full flex-col"
				:class="isModal ? 'min-h-0 flex-1' : 'lg:min-h-0 lg:min-w-0 lg:flex-1'"
			>
				<div
					v-if="saveError || recoverableDraft"
					role="alert"
					class="shrink-0 border-b border-line bg-surface-sunken p-3 text-sm"
				>
					<span>{{
						saveError ||
						'Unsaved changes from an earlier attempt are available.'
					}}</span>
					<button
						v-if="recoverableDraft"
						type="button"
						class="ml-3 underline"
						@click="restoreDraft"
					>
						Restore draft
					</button>
					<button
						v-if="recoverableDraft"
						type="button"
						class="ml-3 underline"
						@click="discardDraft"
					>
						Discard draft
					</button>
					<button
						v-if="saveError"
						type="button"
						class="ml-3 underline"
						@click="saveTask"
					>
						Retry save
					</button>
				</div>
				<!-- HEADER - Fixed at top -->
				<header
					class="flex min-h-[var(--task-header-height)] shrink-0 items-center justify-between gap-2 border-b border-line px-[14px] py-2.5"
				>
					<div class="flex min-w-0 items-center gap-2">
						<DoneCheckbox
							v-if="simpleMode"
							data-tour="task-form.done"
							v-model="doneChecked"
							class="h-7 rounded-pill bg-surface-sunken px-2.5 text-2xs font-bold tracking-wide uppercase hover:bg-surface-hover"
							:disabled="!canMarkDone"
							:tooltip="DONE_DISABLED_TOOLTIP"
						/>
						<Select v-else v-model="statusIdStr">
							<SelectTrigger
								data-tour="task-form.status"
								class="h-7 w-auto gap-1.5 rounded-pill border-0 bg-surface-sunken px-2.5 text-2xs font-bold tracking-wide text-ink-muted uppercase hover:bg-surface-hover"
							>
								<SelectValue placeholder="status" />
							</SelectTrigger>
							<SelectContent class="bg-popover text-popover-foreground">
								<SelectItem
									class="cursor-pointer text-ink hover:!bg-surface-hover hover:!text-ink focus:!bg-surface-hover focus:!text-ink"
									v-for="status in statuses"
									:key="status.id"
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
						<span
							v-if="taskId || form.id"
							class="truncate font-mono text-2xs text-ink-subtle"
						>
							{{ taskDisplayKey }}
						</span>
					</div>

					<div class="flex items-center gap-2">
						<button
							v-if="form.id && isFeatureEnabled('pomodoro')"
							type="button"
							@click="togglePomodoro"
							data-tour="task-form.pomodoro"
							:disabled="pomodoroBusy"
							:class="[
								'relative flex h-7 w-7 items-center justify-center rounded-pill text-sm leading-none transition disabled:opacity-50',
								pomodoroEnabled
									? 'bg-[rgba(232,90,79,0.15)] text-[#e85a4f] hover:bg-[rgba(232,90,79,0.25)]'
									: 'text-ink-subtle hover:bg-surface-hover hover:text-ink',
							]"
							:title="pomodoroEnabled ? 'Disable Pomodoro' : 'Enable Pomodoro'"
						>
							<span aria-hidden="true">🍅</span>
						</button>

						<button
							v-if="canRunWithCursor"
							type="button"
							@click="openCursorAgent"
							class="relative flex h-7 items-center justify-center rounded-pill bg-purple-500/80 px-2 text-xs text-white hover:bg-purple-500 dark:bg-purple-600/70 dark:hover:bg-purple-600/90"
							title="Run with Cursor Agent"
						>
							<Bot class="h-3.5 w-3.5" />
						</button>

						<DropdownMenu
							v-if="
								form.id &&
								(pluginTaskMenuItems.length ||
									canArchiveTask ||
									canUnarchiveTask)
							"
						>
							<DropdownMenuTrigger as-child>
								<button
									type="button"
									class="flex h-7 w-7 items-center justify-center rounded-pill text-ink-subtle hover:bg-surface-hover hover:text-ink"
									aria-label="Task actions"
								>
									<MoreVertical class="size-4" />
								</button>
							</DropdownMenuTrigger>
							<DropdownMenuContent align="end" class="w-56">
								<DropdownMenuItem
									v-if="canUnarchiveTask"
									@click="unarchiveTask"
								>
									<ArchiveRestore class="mr-2 h-4 w-4" />
									<span>Restore</span>
								</DropdownMenuItem>
								<DropdownMenuItem v-if="canArchiveTask" @click="archiveTask">
									<ArchiveIcon class="mr-2 h-4 w-4" />
									<span>Archive</span>
								</DropdownMenuItem>
								<PluginTaskMenuItems :task-id="form.id" :separator="false" />
							</DropdownMenuContent>
						</DropdownMenu>

						<SettingsComponent :form="form" />

						<button
							v-if="isModal"
							@click="emit('close')"
							class="flex h-7 w-7 items-center justify-center rounded-pill text-ink-subtle hover:bg-surface-hover hover:text-ink"
							title="Close (Esc)"
						>
							<XMarkIcon class="size-4" />
						</button>
					</div>
				</header>

				<!-- MAIN - Scrollable content area -->
				<main
					ref="formMain"
					class="flex flex-col gap-5 px-6 pt-5 pb-4"
					:class="
						isModal
							? 'min-h-0 flex-1 overflow-x-hidden overflow-y-auto'
							: 'lg:min-h-0 lg:flex-1 lg:overflow-x-hidden lg:overflow-y-auto'
					"
				>
					<!-- External Update Notification -->
					<Transition name="fade">
						<div
							v-if="hasExternalUpdate"
							class="flex items-center justify-between rounded-lg border border-yellow-300 bg-yellow-50 p-3 dark:border-yellow-600 dark:bg-yellow-900/20"
						>
							<div class="flex items-center gap-2">
								<span class="material-icons text-yellow-500">sync</span>
								<span class="text-sm text-yellow-700 dark:text-yellow-300">
									This task was updated by another user
								</span>
							</div>
							<div class="flex gap-2">
								<button
									@click="applyExternalUpdate"
									class="rounded bg-yellow-500 px-3 py-1 text-xs font-medium text-white hover:bg-yellow-600"
								>
									Apply Changes
								</button>
								<button
									@click="dismissExternalUpdate"
									class="rounded border border-yellow-400 px-3 py-1 text-xs font-medium text-yellow-600 hover:bg-yellow-100 dark:text-yellow-300 dark:hover:bg-yellow-900"
								>
									Dismiss
								</button>
							</div>
						</div>
					</Transition>

					<div
						v-if="(taskId || form.id) && isFeatureEnabled('agent_work')"
						class="mb-1 flex items-center empty:hidden"
					>
						<AgentStatusChip
							:task-id="Number(taskId || form.id)"
							:workspace-id="form.workspace_id"
							hide-when-empty
						/>
					</div>

					<div class="relative">
						<textarea
							data-tour="task-form.title"
							v-model="form.title"
							ref="titleTextarea"
							class="w-full resize-none overflow-hidden border-0 bg-transparent px-0 outline-hidden placeholder:text-ink-subtle"
							:class="[
								'text-2xl leading-snug font-semibold tracking-tight text-ink',
								{ 'text-status-fix-fg': isTitleAtLimit },
							]"
							placeholder="Task name"
							rows="1"
							maxlength="255"
							@input="autoResizeTitle"
							@keydown="onTitleKeydown"
							@beforeinput="onTitleBeforeInput"
							@paste="onTitlePaste"
						/>
						<div
							v-if="isTitleNearLimit"
							class="mt-1 flex items-center gap-1 text-xs"
							:class="isTitleAtLimit ? 'text-red-500' : 'text-orange-400'"
						>
							<span class="material-icons" style="font-size: 14px"
								>warning</span
							>
							<span>{{ form.title?.length || 0 }}/255 characters</span>
						</div>
					</div>

					<TaskPersonChips
						v-if="form.id && pagesVisible"
						:people="form.mentioned_people || []"
					/>

					<!-- Hero TIMER block -->
					<TimeCounter
						v-if="form.id && isFeatureEnabled('task.countdown')"
						data-tour="task-form.timer"
						:form="form"
						:disabled="!form.id"
						@toggle="toggleTimer"
						@update:seconds="updateSeconds"
						@update:common-time="form.common_time = $event"
					/>

					<!-- Pomodoro block (per-task, opt-in) -->
					<PomodoroBlock
						v-if="form.id && isFeatureEnabled('pomodoro')"
						ref="pomodoroBlockRef"
						:task-id="form.id"
						:main-timer-running="mainTimerRunning"
						@request-main-start="handlePomodoroRequestMainStart"
					/>

					<div
						v-if="showLastTaskSettingsPrompt"
						class="flex flex-wrap items-center justify-between gap-2 rounded-md border border-line bg-surface-sunken px-3 py-2 text-sm text-ink-muted"
					>
						<span>Apply settings from the previous task?</span>
						<div class="flex gap-2">
							<Button
								type="button"
								variant="ghost"
								size="sm"
								class="h-8 text-ink-muted hover:bg-surface-hover hover:text-ink"
								@click="lastTaskSettings = null"
							>
								Dismiss
							</Button>
							<Button
								type="button"
								size="sm"
								class="h-8 bg-brand text-on-accent hover:bg-brand-hover"
								@click="applyLastTaskSettings"
							>
								Apply
							</Button>
						</div>
					</div>

					<!-- Properties grid -->
					<div
						class="grid gap-x-4 gap-y-3 text-sm"
						style="
							grid-template-columns: minmax(110px, max-content) minmax(0, 1fr);
						"
					>
						<template v-if="showSurface('assignees.task-row')">
							<div class="flex items-center gap-2 text-ink-subtle">
								<UserIcon class="h-3.5 w-3.5" />
								<span>Assignee</span>
							</div>
							<div class="flex min-w-0 items-center gap-2">
								<AssigneesCombobox
									data-tour="task-form.assignees"
									:assignees="workspaceMembers"
									v-model="assignees as any"
									:assignable-personas="
										isFeatureEnabled('personas') ? assignablePersonas : []
									"
									:selected-personas="personaAssignees"
									@toggle-persona="togglePersona"
								/>
								<PersonaAssigneeChips
									v-if="isFeatureEnabled('personas')"
									:personas="personaAssignees"
									:size="24"
									:member-avatars="memberAvatars"
								/>
								<button
									v-if="!isAssignedToMe"
									type="button"
									@click="assignToMe"
									class="flex h-7 items-center justify-center rounded-pill bg-brand-bg px-2 text-2xs font-semibold text-ink hover:opacity-90"
									title="Assign to me"
								>
									<span class="material-icons" style="font-size: 14px"
										>person_add</span
									>
								</button>
							</div>
						</template>

						<div class="flex items-center gap-2 text-ink-subtle">
							<FlagIcon class="h-3.5 w-3.5" />
							<span id="task-form-priority-label">Priority</span>
						</div>
						<div class="flex min-w-0 items-center">
							<Select v-model="priorityValue">
								<SelectTrigger
									data-testid="task-priority-picker"
									aria-labelledby="task-form-priority-label"
									class="h-7 w-auto gap-1.5 rounded-pill border-0 bg-surface-sunken px-2.5 text-xs text-ink hover:bg-surface-hover"
								>
									<SelectValue />
								</SelectTrigger>
								<SelectContent class="bg-popover text-popover-foreground">
									<SelectItem
										v-for="option in PRIORITIES"
										:key="option.value"
										:value="option.value"
										:show-check-mark="false"
										class="cursor-pointer text-ink hover:!bg-surface-hover hover:!text-ink focus:!bg-surface-hover focus:!text-ink"
									>
										<span class="flex items-center gap-1.5">
											<component
												:is="option.icon"
												class="h-3.5 w-3.5"
												:class="option.colorClass"
												aria-hidden="true"
											/>
											{{ option.label }}
										</span>
									</SelectItem>
								</SelectContent>
							</Select>
						</div>

						<template v-if="showSurface('categories.task-picker')">
							<div class="flex items-center gap-2 text-ink-subtle">
								<FolderIcon class="h-3.5 w-3.5" />
								<span>Category</span>
							</div>
							<div class="flex min-w-0 items-center gap-1.5">
								<CategoriesCombobox
									data-tour="task-form.category"
									class="min-w-0 flex-1"
									:categories="categories"
									v-model="form.project_category_id"
									@update:model-value="
										() => {
											if (!form.title) {
												updateTaskTitle();
											}
											taskKeyNumber = '';
										}
									"
								/>
								<button
									v-if="form.project_category_id"
									type="button"
									class="shrink-0 rounded-md border border-line px-1.5 py-1 text-ink-subtle hover:text-ink"
									:title="`New task in ${
										currentCategoryCode || 'this category'
									}`"
									@click="createAnotherInCategory"
								>
									<PlusIcon class="h-3.5 w-3.5" />
								</button>
							</div>

							<template v-if="form.project_category_id">
								<div class="flex items-center gap-2 text-ink-subtle">
									<HashtagIcon class="h-3.5 w-3.5" />
									<span>Key</span>
								</div>
								<div class="flex min-w-0 items-center gap-1.5">
									<span class="shrink-0 font-mono text-sm text-ink-subtle"
										>{{ currentCategoryCode || 'TASK' }}-</span
									>
									<input
										type="number"
										min="1"
										step="1"
										v-model="taskKeyNumber"
										placeholder="auto"
										class="w-20 min-w-0 rounded-md border border-line bg-surface-sunken px-2 py-1 font-mono text-sm text-ink outline-hidden placeholder:text-ink-subtle focus:border-line-strong"
									/>
									<span
										v-if="isAutoSaving && !taskKeyError"
										class="text-2xs text-ink-subtle"
										>Saving…</span
									>
								</div>
								<div
									v-if="taskKeyError"
									class="col-span-2 -mt-2 text-xs text-status-fix-fg"
								>
									{{ taskKeyError }}
								</div>
							</template>
						</template>

						<template v-if="form.id && isFeatureEnabled('github')">
							<div class="flex items-center gap-2 text-ink-subtle">
								<CodeBracketIcon class="h-3.5 w-3.5" />
								<span>Git activity</span>
							</div>
							<div class="min-w-0">
								<button
									type="button"
									@click="openGitActivity"
									class="inline-flex h-7 items-center gap-1 rounded-pill border border-line bg-surface px-2.5 text-xs font-medium text-ink hover:bg-surface-hover"
								>
									<CodeBracketIcon class="h-3.5 w-3.5" />
									<span>{{
										gitActivityCount > 0
											? `${gitActivityCount} events`
											: 'View activity'
									}}</span>
								</button>
							</div>
						</template>

						<template
							v-if="
								form.id &&
								(isFeatureEnabled('task.relations') ||
									(form.relationTypeWithTask &&
										form.relationTypeWithTask.length > 0))
							"
						>
							<div class="flex items-center gap-2 text-ink-subtle">
								<LinkIcon class="h-3.5 w-3.5" />
								<span>Linked tasks</span>
							</div>
							<div class="flex min-w-0 items-center gap-2">
								<span
									v-if="
										form.relationTypeWithTask &&
										form.relationTypeWithTask.length
									"
									class="text-xs text-ink tabular-nums"
								>
									{{ form.relationTypeWithTask.length }}
									{{
										form.relationTypeWithTask.length === 1 ? 'link' : 'links'
									}}
								</span>
								<button
									type="button"
									@click="openLinkDialog"
									class="inline-flex h-7 items-center gap-1 rounded-pill border border-line bg-surface px-2.5 text-xs font-medium text-ink hover:bg-surface-hover"
								>
									<span class="material-icons" style="font-size: 14px"
										>add</span
									>
									<span>Link</span>
								</button>
							</div>
						</template>

						<template v-if="form.user">
							<div class="flex items-center gap-2 text-ink-subtle">
								<UserCircleIcon class="h-3.5 w-3.5" />
								<span>Author</span>
							</div>
							<div class="flex h-7 min-w-0 items-center text-sm text-ink">
								{{ form.user.name }}
							</div>
						</template>
					</div>

					<!-- DESCRIPTION label -->
					<div
						class="text-2xs font-bold tracking-wide text-ink-subtle uppercase"
					>
						Description
					</div>

					<!-- Editor section with toggle button -->
					<div
						ref="descriptionWrapperRef"
						data-tour="task-form.description"
						class="description-editor-wrapper relative shrink-0 rounded-md border border-line bg-surface-sunken"
						:class="{
							'description-editor-wrapper--editing min-h-[260px]':
								showDescriptionEditor,
						}"
					>
						<DescriptionPreview
							v-if="!showDescriptionEditor"
							:editor-type="editorType"
							:description="form.description"
							:description-json="form.description_json"
							@edit="startDescriptionEdit"
						/>

						<!-- Editor components - no loading state needed since we use localStorage -->
						<Editor
							ref="markdownEditorRef"
							:key="form.id"
							v-else-if="editorType === 'markdown'"
							:model-value="form.description ?? undefined"
							@update:model-value="onDescriptionInput('description', $event)"
							class="mb-0"
						/>

						<BlockEditor
							ref="blockEditorRef"
							:key="form.id"
							v-else-if="editorType === 'block'"
							:model-value="form.description_json"
							@update:model-value="
								onDescriptionInput('description_json', $event)
							"
							placeholder="Type your description here or enter / to see commands or "
							class="block-editor-container mb-0 min-h-[240px] px-2"
						/>

						<BlockMdEditor
							ref="blockMdEditorRef"
							:key="form.id"
							v-else-if="editorType === 'blockmd'"
							:model-value="form.description"
							@update:model-value="onDescriptionInput('description', $event)"
							placeholder="Type your description here or enter / to see commands"
							class="mb-0 min-h-[240px]"
						/>
					</div>

					<div v-if="isDescriptionEditOpen" class="flex justify-end gap-2">
						<Button
							type="button"
							variant="ghost"
							size="sm"
							class="h-8 text-ink-muted hover:bg-surface-hover hover:text-ink"
							@click="cancelDescriptionEdit"
						>
							Cancel
						</Button>
						<Button
							type="button"
							size="sm"
							class="h-8 bg-brand text-on-accent hover:bg-brand-hover"
							@click="finishDescriptionEdit"
						>
							Done
						</Button>
					</div>

					<TaskLinkChips
						v-if="form.id"
						:sources="[form.description, form.description_json]"
					/>

					<div class="flex flex-col gap-0.5 text-2xs text-ink-subtle">
						<div class="flex items-center gap-1">
							<span>Editor: {{ EDITOR_LABELS[editorType] }}</span>
							<span aria-hidden="true">·</span>
							<router-link to="/settings" class="underline hover:text-ink">
								Change in Settings
							</router-link>
						</div>
						<span v-if="showsLegacyEditorNote">
							This task was written in Editor.js and always opens in it.
						</span>
					</div>

					<!-- Task Attachments -->
					<TaskAttachments
						v-if="isFeatureEnabled('task.files')"
						data-tour="task-form.attachments"
						ref="attachmentsRef"
						v-model:pending-files="pendingFiles"
						:task-id="taskId || form.id"
					/>

					<!-- CHECKPOINTS = time log (start -> end, description, duration) -->
					<section
						v-if="isFeatureEnabled('task.checkpoints') && (taskId || form.id)"
						data-tour="task-form.checkpoints"
						class="flex flex-col gap-2"
					>
						<div class="flex items-center justify-between gap-2">
							<div class="flex items-baseline gap-2">
								<span
									class="text-2xs font-bold tracking-wide text-ink-subtle uppercase"
									>Checkpoints</span
								>
								<span
									v-if="checkpointsTotalLabel"
									class="text-2xs text-ink-subtle"
									>· {{ checkpointsTotalLabel }} logged</span
								>
							</div>
							<button
								type="button"
								@click="addCheckpoint"
								class="flex h-7 items-center gap-1 rounded-pill border border-dashed border-line-strong px-2.5 text-2xs font-semibold text-ink-subtle transition hover:bg-surface-sunken hover:text-ink"
							>
								<span class="material-icons" style="font-size: 14px">add</span>
								Add entry
							</button>
						</div>
						<div
							v-if="form.checkpoints && form.checkpoints.length"
							class="overflow-hidden rounded-card border border-line bg-surface"
						>
							<checkpoints :checkpoints="form.checkpoints" />
						</div>
						<div
							v-else
							class="rounded-card border border-dashed border-line-strong px-3 py-3 text-center text-xs text-ink-subtle"
						>
							No time logged yet — click “Add entry” to log what you worked on.
						</div>
					</section>

					<ConfirmForAgentRuns
						v-if="isDesktopApp() && isLocalWorkspaceTask && (taskId || form.id)"
						:task-id="Number(taskId || form.id)"
					/>

					<CloudAgentApproval
						v-if="!isLocalWorkspaceTask && (taskId || form.id)"
						:task-id="Number(taskId || form.id)"
						:workspace-id="
							Number(form.workspace_id || store.getters.currentWorkspaceId)
						"
					/>

					<TaskAgentRunner
						v-if="isDesktopApp() && (taskId || form.id)"
						:task-id="Number(taskId || form.id)"
						:workspace-id="form.workspace_id"
					/>

					<TaskAgentWork
						v-if="(taskId || form.id) && isFeatureEnabled('agent_work')"
						:task-id="Number(taskId || form.id)"
						:workspace-id="form.workspace_id"
					/>

					<TaskTimeEntries
						v-if="!isLocalWorkspaceTask && (taskId || form.id)"
						:task-id="Number(taskId || form.id)"
						:workspace-id="
							Number(form.workspace_id || store.getters.currentWorkspaceId)
						"
					/>

					<PluginTaskSections v-if="form.id" :task="form" />

					<!-- Task Relations -->
					<div
						v-if="
							form.id &&
							(isFeatureEnabled('task.relations') ||
								(form.relationTypeWithTask &&
									form.relationTypeWithTask.length > 0))
						"
					>
						<TaskRelations
							ref="taskRelationsRef"
							:task-id="form.id"
							:relations="form.relationTypeWithTask"
							:hide-header="true"
							@update="reloadTask"
							@open-task="handleOpenLinkedTask"
						/>
					</div>

					<TaskPageMentions
						v-if="form.id && pagesVisible"
						:task-id="Number(form.id)"
					/>

					<EntityGraphSection
						v-if="
							form.id &&
							isFeatureEnabled('graph') &&
							(isFeatureEnabled('task.relations') || pagesVisible)
						"
						:entity="`task:${form.id}`"
					/>

					<!-- Comments (modal: inline at bottom of main; page: in right rail) -->
					<TaskComments
						data-tour="task-form.comments"
						v-if="isModal && form.id && showSurface('comments.task-modal')"
						ref="taskCommentsRef"
						:task-id="form.id"
						class="mt-4"
						@update:count="commentsCount = $event"
					/>
				</main>

				<!-- FOOTER - Fixed at bottom -->
				<footer
					ref="footer"
					class="shrink-0 border-t border-line bg-surface px-6 py-2"
				>
					<!-- Comment composer (modal only — page has it in the right rail) -->
					<AskPersonaButton
						v-if="
							showSurface('comments.composer') &&
							isFeatureEnabled('personas') &&
							isModal &&
							form.id &&
							isLocalWorkspaceTask
						"
						:task-id="form.id"
						@posted="taskCommentsRef?.loadComments()"
					/>
					<div
						v-if="isModal && aiPending && showSurface('comments.composer')"
						class="mb-3 flex [scrollbar-width:none] flex-nowrap items-center gap-2 overflow-x-auto text-xs whitespace-nowrap text-ink-subtle [&::-webkit-scrollbar]:hidden"
					>
						<Loader2 class="h-3.5 w-3.5 shrink-0 animate-spin" />
						<span class="shrink-0"
							>AI is looking around{{ aiPendingSteps.length ? ':' : '…' }}</span
						>
						<span
							v-for="s in aiPendingSteps"
							:key="s.seq"
							class="shrink-0 rounded-pill bg-surface-sunken px-2 py-0.5"
							>{{ agentToolLabel(s.tool) }}</span
						>
					</div>
					<div
						v-if="isModal && form.id && showSurface('comments.composer')"
						class="mb-1.5 flex flex-wrap items-end gap-2 rounded-[18px] border border-line bg-surface-sunken py-1 pr-1.5 pl-4 focus-within:border-line-focus"
						@mousedown.stop
					>
						<CommentAttachments ref="commentAttachments" :task-id="form.id" />
						<textarea
							ref="commentTextarea"
							v-model="newComment"
							rows="1"
							data-esc-blur
							@paste="commentAttachments?.onPaste($event)"
							@dragover.prevent
							@drop="commentAttachments?.onDrop($event)"
							placeholder="Write a comment…"
							class="max-h-32 min-w-0 flex-1 resize-none overflow-y-auto bg-transparent py-1 text-sm leading-5 text-ink outline-hidden placeholder:text-ink-subtle"
							@keydown.enter="onCommentEnter"
							@keydown.esc="blurCommentInput"
						/>
						<MicButton
							v-if="!isLocalWorkspaceTask && isFeatureEnabled('ai.assistant')"
							:key="form.id"
							v-model="newComment"
							:target="commentTextarea"
							:disabled="isSendingComment"
						/>

						<button
							v-if="hasActiveAgent"
							:disabled="!newComment?.trim() || isSendingComment"
							@click="sendToCursor"
							class="flex h-7 w-7 items-center justify-center rounded-pill text-ink-subtle transition hover:bg-surface-hover hover:text-ink disabled:cursor-not-allowed disabled:opacity-40"
							title="Send to Cursor Agent"
						>
							<Bot class="h-4 w-4" />
						</button>
						<button
							v-if="!isLocalWorkspaceTask && isFeatureEnabled('ai.assistant')"
							type="button"
							:aria-pressed="commentAiReply"
							:title="commentAiReply ? 'AI reply: on' : 'AI reply: off'"
							:aria-label="commentAiReply ? 'AI reply: on' : 'AI reply: off'"
							:class="[
								'flex h-7 w-7 items-center justify-center rounded-pill transition',
								commentAiReply
									? 'bg-brand-bg text-brand ring-1 ring-brand'
									: 'text-ink-subtle hover:bg-surface-hover hover:text-ink',
							]"
							@click="toggleCommentAiReply"
						>
							<Sparkles class="h-4 w-4" />
						</button>
						<button
							:disabled="
								!newComment?.trim() || isSendingComment || attachmentsBlocking
							"
							@click="submitComment"
							class="flex h-7 w-7 items-center justify-center rounded-pill bg-brand text-on-accent transition hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-40"
							:title="
								commentAiReply ? 'Ask AI (Enter)' : 'Send comment (Enter)'
							"
						>
							<Send class="h-3.5 w-3.5" />
						</button>
					</div>

					<div class="flex flex-wrap items-center gap-x-2 gap-y-1">
						<Button
							v-if="
								isModal && !detached && isDesktopApp() && (taskId || form.id)
							"
							type="button"
							variant="ghost"
							size="icon"
							data-tour="task-form.open-window"
							title="Open in a separate window"
							aria-label="Open in a separate window"
							class="h-8 w-8 text-ink-muted hover:bg-surface-hover hover:text-ink"
							@click="openInSeparateWindow"
						>
							<ArrowTopRightOnSquareIcon />
						</Button>
						<Button
							v-else-if="isModal && !detached && (taskId || form.id)"
							as="a"
							variant="ghost"
							size="icon"
							:href="generateTaskUrlForAdvancedForm()"
							title="Open advanced form"
							aria-label="Open advanced form"
							class="h-8 w-8 text-ink-muted hover:bg-surface-hover hover:text-ink"
						>
							<ArrowTopRightOnSquareIcon />
						</Button>

						<div
							v-if="taskId || form.id"
							class="flex min-w-0 flex-wrap items-center gap-x-2 text-[11px] text-ink-subtle"
						>
							<span
								v-if="creator"
								data-testid="task-form-creator"
								class="hidden min-w-0 items-center gap-1 sm:flex"
								:title="'Created by ' + creator.name"
							>
								<UserAvatar
									:user-id="creator.id"
									:name="creator.name"
									:has-avatar="!!creator.has_avatar"
									:size="14"
								/>
								<span class="max-w-[12rem] truncate">
									Created by {{ creator.name }}
								</span>
							</span>
							<TaskTimeInfo
								:created-at="form.created_at"
								:updated-at="form.updated_at"
							/>
						</div>

						<div class="ml-auto flex items-center gap-1">
							<TaskShareMenu :task="form" :open-in-app="!isModal" />
							<Button
								v-if="!taskId && !form.id"
								type="button"
								size="sm"
								title="Create"
								class="h-8 bg-brand text-on-accent hover:bg-brand-hover"
								@click="createTask"
							>
								<DocumentPlusIcon />
								Create
							</Button>

							<Button
								v-if="taskId || form.id"
								type="button"
								variant="ghost"
								size="icon"
								title="Save"
								aria-label="Save"
								:aria-busy="isAutoSaving || isLoading"
								:data-unsaved="hasUnsavedChanges || undefined"
								:class="
									hasUnsavedChanges
										? 'h-8 w-8 bg-brand text-on-accent hover:bg-brand-hover hover:text-on-accent'
										: 'h-8 w-8 text-ink-muted hover:bg-surface-hover hover:text-ink'
								"
								@click="saveTask()"
							>
								<Loader2
									v-if="isAutoSaving || isLoading"
									class="animate-spin"
								/>
								<Save v-else />
							</Button>

							<Button
								v-if="isLocalWorkspaceTask && (taskId || form.id)"
								type="button"
								variant="ghost"
								size="icon"
								title="Export to Markdown"
								aria-label="Export to Markdown"
								class="h-8 w-8 text-ink-muted hover:bg-surface-hover hover:text-ink"
								@click="exportTaskToMarkdown"
							>
								<ArrowDownTrayIcon />
							</Button>

							<Button
								v-if="taskId || form.id"
								type="button"
								variant="ghost"
								size="icon"
								title="Delete"
								aria-label="Delete"
								class="h-8 w-8 text-ink-muted hover:bg-status-fix-bg hover:text-status-fix focus-visible:bg-status-fix-bg focus-visible:text-status-fix"
								@click="deleteCurrentTask"
							>
								<TrashIcon />
							</Button>
						</div>
					</div>
				</footer>
			</div>
			<!-- End Form Panel -->

			<!-- RIGHT RAIL — comments (page / non-modal only) -->
			<aside
				v-if="!isModal && form.id && showSurface('comments.rail')"
				:style="footerHeightVars(footerHeight)"
				class="flex w-full flex-col border-t border-line bg-surface lg:h-full lg:w-[380px] lg:shrink-0 lg:border-t-0 lg:border-l xl:w-[420px]"
			>
				<div
					class="flex min-h-[var(--task-header-height)] shrink-0 items-center justify-between border-b border-line px-4 py-3"
				>
					<span class="text-sm font-semibold">Comments</span>
					<span v-if="commentsCount" class="text-xs text-ink-subtle">{{
						commentsCount
					}}</span>
				</div>
				<div
					ref="commentsRailScroller"
					class="px-4 py-4 lg:min-h-0 lg:flex-1 lg:overflow-y-auto"
				>
					<TaskComments
						data-tour="task-form.comments"
						ref="taskCommentsRef"
						:task-id="form.id"
						hide-header
						@update:count="commentsCount = $event"
					/>
				</div>
				<div
					class="flex shrink-0 flex-col justify-center border-t border-line bg-surface px-4 py-3 lg:min-h-[var(--task-footer-height)]"
					@mousedown.stop
				>
					<AskPersonaButton
						v-if="
							isFeatureEnabled('personas') && form.id && isLocalWorkspaceTask
						"
						:task-id="form.id"
						@posted="taskCommentsRef?.loadComments()"
					/>
					<div
						v-if="aiPending"
						class="mb-3 flex [scrollbar-width:none] flex-nowrap items-center gap-2 overflow-x-auto text-xs whitespace-nowrap text-ink-subtle [&::-webkit-scrollbar]:hidden"
					>
						<Loader2 class="h-3.5 w-3.5 shrink-0 animate-spin" />
						<span class="shrink-0"
							>AI is looking around{{ aiPendingSteps.length ? ':' : '…' }}</span
						>
						<span
							v-for="s in aiPendingSteps"
							:key="s.seq"
							class="shrink-0 rounded-pill bg-surface-sunken px-2 py-0.5"
							>{{ agentToolLabel(s.tool) }}</span
						>
					</div>
					<div
						class="flex flex-wrap items-end gap-2 rounded-[18px] border border-line bg-surface-sunken py-1 pr-1.5 pl-4 focus-within:border-line-focus"
					>
						<CommentAttachments ref="commentAttachments" :task-id="form.id" />
						<textarea
							ref="commentRailInput"
							v-model="newComment"
							rows="1"
							data-esc-blur
							@paste="commentAttachments?.onPaste($event)"
							@dragover.prevent
							@drop="commentAttachments?.onDrop($event)"
							placeholder="Write a comment…"
							class="max-h-32 min-w-0 flex-1 resize-none overflow-y-auto bg-transparent py-1 text-sm leading-5 text-ink outline-hidden placeholder:text-ink-subtle"
							@keydown.enter="onCommentEnter"
							@keydown.esc="blurCommentInput"
						/>
						<MicButton
							v-if="!isLocalWorkspaceTask && isFeatureEnabled('ai.assistant')"
							:key="form.id"
							v-model="newComment"
							:target="commentRailInput"
							:disabled="isSendingComment"
						/>
						<button
							v-if="hasActiveAgent"
							:disabled="!newComment?.trim() || isSendingComment"
							@click="sendToCursor"
							class="flex h-7 w-7 items-center justify-center rounded-pill text-ink-subtle transition hover:bg-surface-hover hover:text-ink disabled:cursor-not-allowed disabled:opacity-40"
							title="Send to Cursor Agent"
						>
							<Bot class="h-4 w-4" />
						</button>
						<button
							v-if="!isLocalWorkspaceTask && isFeatureEnabled('ai.assistant')"
							type="button"
							:aria-pressed="commentAiReply"
							:title="commentAiReply ? 'AI reply: on' : 'AI reply: off'"
							:aria-label="commentAiReply ? 'AI reply: on' : 'AI reply: off'"
							:class="[
								'flex h-7 w-7 items-center justify-center rounded-pill transition',
								commentAiReply
									? 'bg-brand-bg text-brand ring-1 ring-brand'
									: 'text-ink-subtle hover:bg-surface-hover hover:text-ink',
							]"
							@click="toggleCommentAiReply"
						>
							<Sparkles class="h-4 w-4" />
						</button>
						<button
							:disabled="
								!newComment?.trim() || isSendingComment || attachmentsBlocking
							"
							@click="submitComment"
							class="flex h-7 w-7 items-center justify-center rounded-pill bg-brand text-on-accent transition hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-40"
							:title="
								commentAiReply ? 'Ask AI (Enter)' : 'Send comment (Enter)'
							"
						>
							<Send class="h-3.5 w-3.5" />
						</button>
					</div>
				</div>
			</aside>
		</div>

		<Confirm
			v-if="showBacklogStatusChangeConfirm"
			title="Task in Backlog"
			:body="backlogConfirmBody"
			@onCancel="
				showBacklogStatusChangeConfirm = false;
				backlogStatusChangeConfirm = null;
			"
			@onOk="handleBacklogStatusChangeConfirm"
		/>

		<Dialog v-model:open="showGitActivityModal">
			<DialogContent class="max-h-[80vh] max-w-3xl overflow-y-auto">
				<DialogHeader>
					<DialogTitle>Git Activity</DialogTitle>
					<DialogDescription class="sr-only"
						>View git commits and pull requests related to this
						task</DialogDescription
					>
				</DialogHeader>
				<TaskGitActivity
					v-if="form.id"
					:task-id="form.id"
					:category-code="currentCategoryCode || 'TASK'"
					@update:count="gitActivityCount = $event"
				/>
			</DialogContent>
		</Dialog>

		<Dialog v-model:open="showCursorAgentModal">
			<DialogContent
				class="max-h-[80vh] max-w-3xl overflow-y-auto rounded-card border border-line bg-surface text-ink"
			>
				<DialogHeader>
					<DialogTitle class="text-ink">Cursor AI Agent</DialogTitle>
					<DialogDescription class="sr-only"
						>Run AI agent to help with this task</DialogDescription
					>
				</DialogHeader>
				<TaskCursorAgent
					v-if="form.id"
					:task-id="form.id"
					@launched="loadCursorAgents"
					@stopped="loadCursorAgents"
					@followup-sent="taskCommentsRef?.loadComments()"
				/>
			</DialogContent>
		</Dialog>

		<Dialog
			:open="!!integrationHint"
			@update:open="
				(value) => {
					if (!value) integrationHint = null;
				}
			"
		>
			<DialogContent
				class="max-w-md rounded-card border border-line bg-surface text-ink"
			>
				<DialogHeader>
					<DialogTitle class="text-ink">{{
						integrationHint?.title
					}}</DialogTitle>
					<DialogDescription class="text-ink-subtle">
						{{ integrationHint?.message }}
					</DialogDescription>
				</DialogHeader>
				<div class="mt-4 flex flex-wrap justify-end gap-2">
					<button
						type="button"
						class="h-9 rounded-pill border border-line bg-surface px-4 text-sm font-medium text-ink hover:bg-surface-hover"
						@click="integrationHint = null"
					>
						Not now
					</button>
					<button
						type="button"
						class="h-9 rounded-pill bg-brand px-4 text-sm font-semibold text-on-accent shadow-tmgr-xs transition-opacity hover:opacity-90"
						@click="goToCategorySettings"
					>
						{{ integrationHint?.ctaLabel }}
					</button>
				</div>
			</DialogContent>
		</Dialog>
		<div
			v-if="isFileDragActive || isUploadingPendingFiles"
			class="absolute inset-0 z-50 rounded-panel border-2 border-dashed border-brand bg-[color-mix(in_srgb,var(--brand-bg-color)_80%,transparent)]"
			:class="{ 'pointer-events-none': !isUploadingPendingFiles }"
		>
			<div
				class="sticky top-0 flex h-full max-h-[100dvh] flex-col items-center justify-center gap-2 text-brand"
			>
				<template v-if="isUploadingPendingFiles">
					<Loader2 :size="32" class="animate-spin" />
					<span class="text-lg font-semibold">Uploading files…</span>
				</template>
				<template v-else>
					<Upload :size="32" />
					<span class="text-lg font-semibold">Drop it here to upload</span>
				</template>
			</div>
		</div>
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
		min-height: 200px !important;
		width: 100% !important;
		box-sizing: border-box;
		position: relative !important;
	}

	/* Improve list display in modal windows */
	.block-editor-container .ce-block {
		max-width: 100%;
		padding: 0 4px;
		box-sizing: border-box;
	}

	/* Fix for modals to ensure content is visible and scrollable */
	.new-form-container.h-full .block-editor-container {
		overflow-y: visible;
		contain: none;
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
	.cdx-list__item {
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

	/* Description editor wrapper (modal/side-panel) — no overflow clipping so editor popups (slash menu, toolbox) can overflow; the panel scrolls */
	.description-editor-wrapper--editing .md-editor:not(.md-editor-fullscreen) {
		border: 0 !important;
		background: transparent !important;
		/* md-editor-v3 defaults to 500px and scrolls internally; the wrapper no longer clips it */
		height: 420px !important;
	}
</style>
