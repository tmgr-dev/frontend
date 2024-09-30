<template>
	<teleport to="title">{{ form.title }}&nbsp;</teleport>

	<div
		class="flex h-full flex-col gap-4 overflow-y-auto rounded-lg p-6"
		:class="[isModal ? 'md:w-[700px]' : 'container mx-auto']"
	>
		<div class="flex items-center justify-between">
			<HeadlessSelect
				:options="statuses"
				label="name"
				v-model="selectedStatus"
				placeholder="status"
				class="w-40"
			>
				<template #beforeSelectedValue>
					<span
						v-if="selectedStatus"
						class="mr-3 inline-block size-2 shrink-0 rounded-full"
						:style="{ backgroundColor: selectedStatus?.color }"
					/>
				</template>

				<template #beforeOption="{ option }">
					<span
						class="mr-3 inline-block size-2 shrink-0 rounded-full"
						:style="{ backgroundColor: option.color }"
					/>
				</template>
			</HeadlessSelect>

			<button v-if="isModal" @click="$emit('close')">
				<XMarkIcon
					class="h-5 w-5 fill-neutral-600 hover:fill-black dark:hover:fill-white"
				/>
			</button>
		</div>

		<div class="flex items-center justify-between">
			<TextField
				v-model="form.title"
				class="w-full"
				input-class="w-full text-lg font-bold border-0 !px-0 !bg-transparent"
				placeholder="Task name"
			/>
		</div>

		<div class="grid items-center gap-4 md:grid-cols-3">
			<div class="flex items-center gap-2">
				<div class="flex rounded bg-[#F4CD48]">
					<PlantIcon class="m-auto h-10 w-10" />
				</div>

				<div class="w-full">
					<div class="text-xs text-neutral-400">Category</div>

					<Select
						:options="[
							{ title: 'Category 1', value: 1 },
							{ title: 'Category 2', value: 2 },
						]"
						label-key="title"
						value-key="value"
					/>
				</div>
			</div>

			<div class="flex items-center gap-2">
				<div class="flex rounded border border-neutral-200">
					<ProfileIcon class="size-10" />
				</div>

				<div>
					<div class="text-xs text-neutral-400">Assignees</div>
					<div class="text-sm text-gray-600">savayer, +2</div>
				</div>
			</div>

			<!--			<div class="flex items-center gap-2">
				<div class="flex rounded border border-neutral-200">
					<CalendarIcon class="h-8 w-8" />
				</div>

				<div>
					<div class="text-xs text-neutral-400">Due date</div>
					<div class="text-sm text-gray-600">05 Nov</div>
				</div>
			</div>-->
		</div>

		<Editor v-model="form.description" class="mb-2 min-h-60 md:!h-72" />

		<!--	actions	-->
		<footer ref="footer" class="shadow-top z-10 mt-auto w-full rounded-lg">
			<div class="flex justify-end gap-3 text-center">
				<button
					v-if="taskId"
					@click="removeTask"
					class="mr-auto rounded bg-red-500 px-4 py-2 font-bold text-white outline-none hover:bg-red-700"
				>
					Delete
				</button>

				<!--				<span
					v-if="form.approximately_time"
					:class="`text-${
						approximatelyEndTime === '00:00' ? 'red' : 'gray'
					}-500 estimated-info hidden py-2 pr-5 md:block`"
				>
					Left time: {{ approximatelyEndTime }}
				</span>-->

				<span class="relative inline-flex rounded-md shadow-sm">
					<button
						v-if="taskId"
						@click="saveTask"
						class="relative rounded bg-blue-500 px-8 py-2 font-bold text-white hover:bg-blue-700 focus:outline-none"
						type="button"
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
						<span>Save</span>
					</button>
				</span>

				<button
					v-if="!taskId"
					@click="createTask"
					class="mb-5 rounded bg-orange-500 px-4 py-2 font-bold text-white transition hover:bg-orange-600 focus:outline-none sm:mb-0"
					type="button"
				>
					Create
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
					@click="openSettings"
					class="rounded bg-gray-500 px-4 py-2 font-bold text-white hover:bg-gray-700 focus:outline-none"
					type="button"
				>
					Settings
				</button>
			</div>
		</footer>
	</div>

	<!--
	<button
		@click="toggleChat"
		class="absolute top-1/2 right-4 -translate-y-1/2 transform rounded-full bg-blue-500 p-2 text-white transition-colors hover:bg-blue-600"
	>
		<svg
			xmlns="http://www.w3.org/2000/svg"
			class="h-6 w-6"
			fill="none"
			viewBox="0 0 24 24"
			stroke="currentColor"
		>
			<path
				stroke-linecap="round"
				stroke-linejoin="round"
				stroke-width="2"
				d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"
			/>
		</svg>
	</button>-->
</template>

<script setup lang="ts">
	import Multiselect from 'vue-multiselect';
	import { XMarkIcon } from '@heroicons/vue/20/solid';
	import store from 'src/store';
	import { computed, onBeforeMount, reactive, ref } from 'vue';
	import { getWorkspaceMembers } from 'src/actions/tmgr/workspaces';
	import { useRoute } from 'vue-router';
	import {
		createTask as createTaskAction,
		getTask,
		Task,
	} from 'src/actions/tmgr/tasks';
	import PlantIcon from 'src/components/icons/PlantIcon.vue';
	import Select from 'src/components/general/Select.vue';
	import ProfileIcon from 'src/components/icons/ProfileIcon.vue';
	import CalendarIcon from 'src/components/icons/CalendarIcon.vue';
	import Editor from 'src/components/Editor.vue';
	import TextField from 'src/components/general/TextField.vue';
	import TaskActions from 'src/components/tasks/TaskActions.vue';
	import { getStatuses, Status } from 'src/actions/tmgr/statuses';
	import 'vue-multiselect/dist/vue-multiselect.css';
	import HeadlessSelect from 'src/components/general/HeadlessSelect.vue';

	interface Props {
		isModal: boolean;
	}

	const props = defineProps<Props>();
	const route = useRoute();
	let form = reactive<Partial<Task>>({});
	const modalTaskId = computed(() => store.state.currentTaskIdForModal);
	const statusId = computed(() => store.state.taskStatusId);
	const modalProjectCategoryId = computed(
		() => store.state.createTaskInProjectCategoryId,
	);
	const projectCategoryId = computed(() =>
		form?.id
			? null
			: route.params.project_category_id || modalProjectCategoryId.value,
	);
	const taskId = computed(
		() => modalTaskId.value || (route.params.id as string),
	);
	const statuses = ref<Status[]>();
	const selectedStatus = ref<Status>();

	async function initComponent() {
		statuses.value = await getStatuses();
		console.log(statuses.value);
		if (taskId) {
			form = await getTask(+taskId);
			/*if (form.workspace_id) {
				workspaceMembers = await getWorkspaceMembers(form.workspace_id);
			}*/
			// this.workspaceMembers = await getWorkspaceMembers(this.form.workspace_id);
			// window.onkeydown = this.getShortcutSaveListener();
		}

		/*if (this.projectCategoryId && this.isCreatingTask) {
			this.form.project_category_id = this.projectCategoryId;
		}
		await this.loadCategory();
		await this.loadTaskSettings();
		this.$store.getters.getPusher
			.private(`App.Workspace.${this.form.workspace_id}`)
			.on('comment-added', ({ comment }) => {
				if (this.taskId === comment.task_id) {
					this.$refs.commentsChat.addingComment(comment);
				}
			})
			.on('comment-updated', ({ comment }) => {
				if (this.taskId === comment.task_id) {
					this.$refs.commentsChat.editingComment(comment);
				}
			})
			.on('comment-deleted', ({ comment }) => {
				if (this.taskId === comment.task_id) {
					this.$refs.commentsChat.deletingComment(comment);
				}
			});

		this.$store.getters.getPusher
			.private(`App.User.${this.$store.state.user.id}`)
			.on('task-countdown-stopped', ({ task }) => {
				const isCountdownStarted = !!this.form.start_time;
				if (!isCountdownStarted) {
					return;
				}

				this.setFormDataWithDelay(task);
			})
			.on('task-countdown-started', ({ task }) => {
				const isCountdownStarted = !!this.form.start_time;
				if (!isCountdownStarted) {
					this.setFormDataWithDelay(task);
				}
			});*/
	}

	onBeforeMount(() => {
		initComponent();
	});

	const isChatOpen = ref(false);
	const chatMessages = ref([
		{ sender: 'System', text: 'Welcome to the chat!' },
		{ sender: 'User', text: 'Hello, I have a question about the task.' },
	]);
	const newMessage = ref('');
	const toggleChat = () => {
		isChatOpen.value = !isChatOpen.value;
	};
	const sendMessage = () => {
		if (newMessage.value.trim()) {
			chatMessages.value.push({ sender: 'User', text: newMessage.value });
			newMessage.value = '';
		}
	};

	const createTask = async () => {
		form.approximately_time = form.settings?.find(
			(item: any) => item.key === 'approximately_time',
		)?.value;
		// @ts-ignore @todo fix types
		form = await createTaskAction(form);
		/*if (!taskId) {
			showAlert();
		}*/
	};

	const removeTask = () => {};
	const saveTask = () => {};
	const openSettings = () => {};
</script>
