<template>
	<div class="relative w-full items-center justify-center">
		<Transition name="bounce">
			<div>
				<TasksMultipleActionsModal
					v-if="isShowSelectedTasksCommonTime"
					:is-loading-actions="loadingActionsForMultipleTasks"
					:status="status"
					@close="closeTimeInModal"
					@export="exportSelectedTasks"
					@remove="deleteSelectedTasks"
					@updateStatus="updateStatusForSelectedTasks"
				>
					<p class="text-center text-white">
						<b>Selected tasks: </b>{{ selected.filter(Boolean).length }}<br />
						{{ timeForModal }}
					</p>
				</TasksMultipleActionsModal>
			</div>
		</Transition>

		<div
			v-selectable="
				hasSelectable ? { selectedGetter, selectedSetter, selectingSetter } : {}
			"
			class="relative"
		>
			<div
				class="border-black dark:border-gray-400"
				:class="[hasSelectable ? 'selection' : '']"
			/>

			<div
				v-for="(task, i) in tasks"
				:key="i"
				:class="{
					selected: !!selected[i],
					selecting: !!selecting[i],
					'opacity-50 hover:opacity-100': task.deleted_at,
				}"
				:data-task-id="task.id"
				:draggable="draggable"
				class="selectable relative mt-2 w-full rounded-lg px-2"
				@dragstart="onDragStart($event, task)"
			>
				<BounceLoader
					v-if="loadingActionTasksIds.includes(task.id)"
					class="l-0 t-0 absolute z-10"
				/>

				<div
					:class="{
						'border-l-8 border-solid border-green-600': task.start_time,
					}"
					class="rounded-lg shadow-md md:flex"
				>
					<div
						class="relative flex w-full items-center rounded-lg bg-white p-4 transition-colors duration-300 hover:bg-gray-100 dark:bg-gray-900 hover:dark:bg-gray-800"
					>
						<div class="flex flex-col gap-1">
							<CategoryBadge
								v-if="showCategoryBadges"
								class="shrink-0 self-start"
								:category="task.category"
							/>

							<TaskMeta
								class="max-w-xl xl:max-w-3xl 2xl:max-w-4xl"
								:show-category-badges="showCategoryBadges"
								:task="task"
								:task-time="getTaskFormattedTime(task)"
								@openTask="$store.commit('setCurrentTaskIdForModal', task.id)"
							/>
						</div>

						<div class="ml-auto">
							<TaskButtonsInTheList
								:is-loading-actions="isLoadingActions"
								:showed-buttons="getShowButtons(task)"
								:task="task"
								@startCountdown="startCountdown"
								@stopCountdown="stopCountdown"
								@updateStatus="updateStatus"
							/>
						</div>
					</div>
				</div>
			</div>
		</div>
	</div>

	<Confirm
		v-if="confirm"
		:body="confirm.body"
		:title="confirm.title"
		@onCancel="confirm = undefined"
		@onOk="confirm.action()"
	/>
</template>

<script>
	import downloadFile from '@/utils/downloadFile';
	import Loader from '@/components/loaders/Loader.vue';
	import Confirm from '@/components/general/Confirm.vue';
	import TasksListMixin from '@/mixins/TasksListMixin';
	import TaskMeta from '@/components/tasks/TaskMeta.vue';
	import BounceLoader from '@/components/loaders/BounceLoader.vue';
	import DropdownMenu from '@/components/general/DropdownMenu.vue';
	import TaskActionsInTheListMixin from '@/mixins/TaskActionsInTheListMixin';
	import TaskButtonsInTheList from '@/components/tasks/TaskButtonsInTheList.vue';
	import TasksMultipleActionsModal from '@/components/tasks/TasksMultipleActionsModal.vue';
	import Modal from '@/components/Modal.vue';
	import TaskForm from '@/pages/TaskForm.vue';
	import {
		exportTasks,
		getTask,
		startTaskTimeCounter,
		stopTaskTimeCounter,
		updateTaskStatus,
	} from '@/actions/tmgr/tasks';
	import CategoryBadge from '@/components/general/CategoryBadge.vue';

	export default {
		name: 'TasksListComponent',
		components: {
			CategoryBadge,
			TaskForm,
			Modal,
			Confirm,
			BounceLoader,
			Loader,
			TaskMeta,
			TasksMultipleActionsModal,
			TaskButtonsInTheList,
			DropdownMenu,
		},
		emits: ['reload-tasks'],
		props: {
			tasks: {
				required: false,
				type: Array,
				default: () => [],
			},
			status: {
				required: false,
				type: String,
				default: null,
			},
			useTaskStatusForButtons: {
				required: false,
				type: Boolean,
				default: false,
			},
			showCategoryBadges: {
				required: false,
				type: Boolean,
				default: true,
			},
			isLoadingActions: {
				required: true,
				type: Object,
			},
			draggable: {
				type: Boolean,
				required: false,
				default: false,
			},
			hasSelectable: {
				type: Boolean,
				required: false,
				default: false,
			},
			loadingActionTasksIds: {
				type: Array,
				required: false,
				default: () => [],
			},
		},
		mixins: [TasksListMixin, TaskActionsInTheListMixin],
		data: () => ({
			showTaskForm: false,
			modalTaskId: null,
			confirm: null,
			isShowSelectedTasksCommonTime: false,
			selected: [],
			selecting: [],
			showTimeInModal: false,
			timeForModal: null,
			loadingActionsForMultipleTasks: [],
		}),
		methods: {
			closeTaskModal() {
				this.$store.commit('closeTaskModal');
			},
			async stopCountdown(task, dotId) {
				this.isLoadingActions[dotId] = true;
				await stopTaskTimeCounter(task.id);
				await this.loadTasks();
				this.isLoadingActions[dotId] = false;
			},
			async startCountdown(task, dotId) {
				this.isLoadingActions[dotId] = true;
				await startTaskTimeCounter(task.id);
				await this.loadTasks();
				this.isLoadingActions[dotId] = false;
			},
			onDragStart(event, task) {
				event.dataTransfer.setData('task-id', task.id);
			},
			loadTasks() {
				this.$emit('reload-tasks');
			},
			getShowButtons(task) {
				return {
					start: task.start_time,
					stop: !task.start_time,
					deleteTask: this.status === 'hidden' || this.status === 'done',
				};
			},
			async updateStatus(task, status, dotId = null, loadTasks = true) {
				try {
					this.setLoadingAction(dotId);
					await updateTaskStatus(task.id, status);

					if (loadTasks) {
						this.loadTasks();
					}
				} catch (e) {
					console.error(e);
				} finally {
					this.setLoadingAction(dotId, false);
				}
			},
			async updateStatusForSelectedTasks(status) {
				this.loadingActionsForMultipleTasks.push(status);
				for (let i = 0; i < this.tasks.length; ++i) {
					if (!this.selected[i]) {
						continue;
					}
					await this.updateStatus(this.tasks[i], status, null, false);
				}
				await this.loadTasks();
				this.loadingActionsForMultipleTasks =
					this.loadingActionsForMultipleTasks.filter((s) => s !== status);
				this.resetSelectedTasks();
			},
			showConfirm(title, body, action) {
				this.confirm = { title, body, action };
			},
			deleteSelectedTasks() {
				const deleteMultipleTasks = async () => {
					try {
						this.loadingActionsForMultipleTasks.push('delete');
						for (let i = 0; i < this.tasks.length; ++i) {
							if (!this.selected[i]) {
								continue;
							}
							const data = await getTask(this.tasks[i].id);

							this.tasks[i].deleted_at = data.deleted_at;
						}
					} catch (e) {
						console.error(e);
					} finally {
						this.confirm = null;
						this.loadingActionsForMultipleTasks =
							this.loadingActionsForMultipleTasks.filter((s) => s !== 'delete');
						this.resetSelectedTasks();
					}
				};
				this.showConfirm('Delete tasks', 'Are you sure?', deleteMultipleTasks);
			},
			selectedGetter() {
				return this.selected;
			},
			selectingSetter(arr) {
				if (arr.filter((item) => item).length >= 2) {
					this.selecting = arr;
				}
			},
			selectedSetter(arr) {
				if (!this.selected.filter(Boolean).length && !this.selecting.length) {
					return;
				}
				this.selected = arr.map((v, i) =>
					this.selected[i] && v
						? false
						: v && !this.selected[i]
						? true
						: !v && this.selected[i],
				);

				this.isShowSelectedTasksCommonTime =
					this.selected.filter(Boolean).length > 1;
				this.selecting = [];
				if (this.isShowSelectedTasksCommonTime) {
					this.countTimeForModal();
				}
			},
			countTimeForModal() {
				const time = this.getSelectedTasks().reduce(
					(p, c) => p + c.common_time,
					0,
				);
				this.timeForModal = this.formatTime(time);
			},
			getSelectedTasks() {
				return this.tasks.filter((task, index) => this.selected[index]);
			},
			resetSelectedTasks() {
				this.isShowSelectedTasksCommonTime = false;
				this.selected = [];
				this.selecting = [];
			},
			closeTimeInModal() {
				this.showTimeInModal = false;
				this.isShowSelectedTasksCommonTime = false;
				this.resetSelectedTasks();
			},
			async exportSelectedTasks(exportType = 'csv') {
				this.loadingActionsForMultipleTasks.push(exportType);
				const tasksIds = this.getSelectedTasks().map(({ id }) => id);

				try {
					const data = await exportTasks(exportType, {
						ids: tasksIds,
						per_hour: 1000,
					});

					downloadFile(data, 'export.' + exportType);
				} catch (e) {
					console.error(e);
				} finally {
					this.loadingActionsForMultipleTasks =
						this.loadingActionsForMultipleTasks.filter(
							(type) => type !== exportType,
						);
				}
			},
			selectAll() {
				this.selected = this.tasks.map(() => true);
				this.isShowSelectedTasksCommonTime =
					this.selected.filter(Boolean).length > 1;

				this.countTimeForModal();
			},
		},
	};
</script>
