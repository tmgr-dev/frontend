<template>
	<teleport to="title">{{ form.title }}&nbsp;</teleport>

	<div
		class="flex h-full flex-col gap-4 overflow-y-auto rounded-lg p-6"
		:class="[isModal ? 'md:w-[700px]' : 'container mx-auto pt-14']"
	>
		<header class="flex justify-between">
			<Select v-model="selectedStatus">
				<SelectTrigger class="w-40 border-0">
					<SelectValue placeholder="status" />
				</SelectTrigger>
				<SelectContent class="border-0 bg-white dark:bg-gray-800">
					<SelectItem
						class="cursor-pointer text-gray-900 hover:bg-tmgr-light-blue hover:!text-white dark:text-gray-400"
						v-for="status in statuses"
						:value="status"
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

			<button v-if="isModal" @click="$emit('close')">
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

		<!--			<Countdown :init-task="form" :disabled="!form.id" />-->

		<div class="grid grid-cols-2 gap-4 md:flex md:items-center">
			<CategoriesCombobox
				:categories="categories"
				v-model="form.project_category_id"
			/>

			<AssigneesCombobox :assignees="workspaceMembers" />

			<div class="ml-auto">
				<TimeCounter v-if="taskId" :init-task="form" :disabled="!form.id" />
			</div>
		</div>

		<Editor
			v-model="form.description"
			class="mb-2 grow md:h-72"
			:class="[!isModal && 'lg:min-h-96']"
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
	import {
		XMarkIcon,
		TrashIcon,
		DocumentPlusIcon,
		BookmarkIcon,
	} from '@heroicons/vue/20/solid';
	import { ArrowTopRightOnSquareIcon } from '@heroicons/vue/24/outline';
	import store from 'src/store';
	import { computed, onBeforeMount, reactive, ref, toRef } from 'vue';
	import { useRoute } from 'vue-router';
	import {
		createTask as createTaskAction,
		getTask,
		Task,
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

	interface Props {
		isModal: boolean;
	}

	const props = defineProps<Props>();
	const route = useRoute();
	// @todo find out why v-model doesn't work with reactive
	//let form = reactive<Partial<Task>>({});
	let form = ref<Partial<Task>>({});
	const modalTaskId = toRef(store.state, 'currentTaskIdForModal');
	const statusId = computed(() => store.state.taskStatusId);
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
	const selectedStatus = ref<Status>();
	const categories = ref<Category[]>([]);
	const workspaceMembers = ref<WorkspaceMember[]>([]);

	async function initComponent() {
		const [loadedStatuses, loadedCategories] = await Promise.all([
			getStatuses(),
			getCategories(),
		]);
		statuses.value = loadedStatuses;
		categories.value = loadedCategories;

		if (taskId.value) {
			if (props.isModal) {
				history.pushState({}, '', `/${taskId.value}`);
			}

			form.value = await getTask(+taskId.value);
			if (form.value.workspace_id) {
				workspaceMembers.value = await getWorkspaceMembers(
					form.value.workspace_id,
				);
			}
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

	const createTask = async () => {
		form.value.approximately_time = form.value.settings?.find(
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
</script>
