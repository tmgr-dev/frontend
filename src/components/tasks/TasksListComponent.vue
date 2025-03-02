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
			:key="hasSelectable"
			v-selectable="
				hasSelectable ? { selectedGetter, selectedSetter, selectingSetter } : {}
			"
			class="relative border pb-2"
			:class="[
				hasSelectable
					? 'rounded-2xl border-dashed border-orange-200 px-2'
					: 'border-transparent',
			]"
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

				<button
					type="button"
					:class="{
						'border-l-8 border-solid border-green-600': task.start_time,
					}"
					class="w-full rounded-lg shadow-md md:flex"
					@click="$store.commit('setCurrentTaskIdForModal', task.id)"
				>
					<div
						class="w-full space-y-1 rounded-lg bg-white p-4 transition-colors duration-300 hover:bg-gray-100 dark:bg-gray-900 hover:dark:bg-gray-800"
					>
						<CategoryBadge
							v-if="showCategoryBadges"
							class="shrink-0 self-start"
							:category="task.category"
						/>

						<div class="text-left text-sm font-medium lg:text-lg">
							{{ task.title }}
						</div>

						<div class="flex items-center gap-2">
							<span
								:class="task.start_time ? 'text-green-600' : 'text-orange-600'"
								class="material-icons text-xs sm:text-base md:text-xl"
							>
								alarm
							</span>

							<span class="text-xs text-gray-700 sm:text-base md:text-base">
								{{ getTaskFormattedTime(task) }}
							</span>
						</div>
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
				</button>
			</div>

			<!-- Add pagination controls -->
			<div v-if="pagination.total > pagination.per_page" class="mt-4 flex items-center justify-between px-4">
				<div class="flex items-center gap-2">
					<span class="text-sm text-gray-600">
						Showing {{ pagination.from }} to {{ pagination.to }} of {{ pagination.total }} tasks
					</span>
					
					<select 
						v-model="perPage" 
						@change="onPerPageChange"
						class="rounded-md border border-gray-300 bg-white px-2 py-1 text-sm"
					>
						<option :value="10">10 per page</option>
						<option :value="25">25 per page</option>
						<option :value="50">50 per page</option>
					</select>
				</div>
				
				<div class="flex items-center gap-2">
					<button
						:disabled="pagination.current_page === 1"
						@click="onPageChange(pagination.current_page - 1)"
						class="rounded-md border border-gray-300 bg-white px-3 py-1 text-sm disabled:opacity-50"
					>
						Previous
					</button>
					
					<span class="text-sm text-gray-600">
						Page {{ pagination.current_page }} of {{ pagination.last_page }}
					</span>
					
					<button
						:disabled="pagination.current_page === pagination.last_page"
						@click="onPageChange(pagination.current_page + 1)"
						class="rounded-md border border-gray-300 bg-white px-3 py-1 text-sm disabled:opacity-50"
					>
						Next
					</button>
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

<script lang="ts">
	import downloadFile from '@/utils/downloadFile';
	import Loader from '@/components/loaders/Loader.vue';
	import Confirm from '@/components/general/Confirm.vue';
	import TasksListMixin from '@/mixins/TasksListMixin';
	import BounceLoader from '@/components/loaders/BounceLoader.vue';
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
		Task,
		PaginationMeta,
	} from '@/actions/tmgr/tasks';
	import CategoryBadge from '@/components/general/CategoryBadge.vue';
	import Button from '@/components/general/Button.vue';
	import { PropType } from 'vue';

	export default {
		name: 'TasksListComponent',
		components: {
			Button,
			CategoryBadge,
			TaskForm,
			Modal,
			Confirm,
			BounceLoader,
			Loader,
			TasksMultipleActionsModal,
			TaskButtonsInTheList,
		},
		emits: ['reload-tasks', 'page-change', 'per-page-change'],
		props: {
			tasks: {
				type: Array as PropType<Task[]>,
				required: false,
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
			pagination: {
				type: Object as PropType<PaginationMeta>,
				required: true,
				default: () => ({
					current_page: 1,
					per_page: 10,
					total: 0,
					last_page: 1,
					from: 0,
					to: 0
				})
			}
		},
		watch: {
			hasSelectable(v) {
				if (!v) {
					this.selected = [];
					this.isShowSelectedTasksCommonTime = false;
				}
			},
			'pagination.per_page': {
				immediate: true,
				handler(value) {
					this.perPage = value;
				}
			}
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
			perPage: 10
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
				this.selected = arr.map((v, i) =>
					this.selected[i] && v
						? false
						: v && !this.selected[i]
						? true
						: !v && this.selected[i],
				);

				this.isShowSelectedTasksCommonTime =
					this.selected.filter(Boolean).length >= 1;
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
			onPageChange(page: number) {
				if (page < 1 || page > this.pagination.last_page) return;
				this.$emit('page-change', page);
				this.resetSelectedTasks();
			},
			onPerPageChange() {
				if (this.perPage === this.pagination.per_page) return;
				this.$emit('per-page-change', this.perPage);
				this.resetSelectedTasks();
			}
		},
	};
</script>
