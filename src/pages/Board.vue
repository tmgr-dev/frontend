<template>
	<Teleport to="title">{{ title }}</Teleport>

	<BaseLayout no-copyright :body-container-class="''">
		<template #body>
			<div class="flex flex-col justify-center flex-1 min-h-0">
				<div class="w-full px-4 py-2 mb-2">
					<WorkspaceUsers
						:users="workspaceUsersWithoutAll"
						:workspace-id="workspaceId"
					/>
				</div>
				<div class="w-full overflow-x-auto flex flex-col h-full min-h-0">
					<div class="min-h-[62px] flex-shrink-0">
						<div class="relative md:hidden">
							<div
								class="fixed right-0 z-40 -mt-16 mr-auto flex items-center justify-center p-4 pt-5"
							>
								<span
									class="material-icons cursor-pointer duration-300 ease-in-out hover:scale-95 hover:text-blue-200"
									@click="
										async () => {
											await loadColumns();
											await loadTasks();
										}
									"
								>
									refresh
								</span>
							</div>

							<Transition name="bounce-right-fade">
								<Modal
									v-if="isFiltersModalShown"
									modal-class="p-6 w-96"
									close-on-bg-click
									@close="isFiltersModalShown = false"
								>
									<template #modal-body>
										<FiltersBoard
											v-if="workspaceUsers.length"
											:workspaceUsers="workspaceUsers"
											:categories="categories"
											:chosen-user.sync="chosenUser"
											@update:chosenUser="handleChosenUserUpdate"
											@handleChosenCategory="handleChosenCategory"
											:activeDraggable="activeDraggable"
											@handleUpdateDraggable="handleUpdateDraggable"
											@handleSearchTextChanged="handleSearchTextChanged"
											@loadTasks="loadTasks"
											@loadColumns="loadColumns"
										/>
									</template>
								</Modal>
							</Transition>
						</div>

						<div class="hidden items-center md:flex">
							<FiltersBoard
								v-if="workspaceUsers.length"
								:workspaceUsers="workspaceUsers"
								:categories="categories"
								:chosen-user.sync="chosenUser"
								@update:chosenUser="handleChosenUserUpdate"
								@handleChosenCategory="handleChosenCategory"
								:activeDraggable="activeDraggable"
								@handleUpdateDraggable="handleUpdateDraggable"
								@handleSearchTextChanged="handleSearchTextChanged"
								@loadTasks="loadTasks"
								@loadColumns="loadColumns"
							>
								<template #actions-start>
									<!-- Add Status Icon Button -->
									<button
										@click="openCreateStatusModal"
										class="mr-4 cursor-pointer text-gray-500 hover:text-black dark:text-gray-600 dark:hover:text-white"
										title="Add Status"
									>
										<span class="cursor-pointer material-icons text-2xl">add</span>
									</button>
									<!-- End Add Status Icon Button -->
								</template>
							</FiltersBoard>
						</div>
					</div>

					<div class="board-container">
						<div class="w-fit" ref="cont1">
							<Draggable
								ref="cont2"
								:disabled="!activeDraggable"
								v-model="columns"
								group="columns"
								item-key="id"
								class="board-container"
								@end="onMove"
								data-id="column"
							>
								<template #item="{ element: column }">
									<div class="board-container__item pr-2">
										<div class="column-width h-full rounded-lg px-3 pb-3 flex flex-col">
											<div class="flex-shrink-0">
												<div
													class="relative flex items-center rounded pb-2 pl-2 pt-2 font-sans text-sm font-semibold tracking-wide text-tmgr-blue dark:text-tmgr-gray"
													:style="{
														'border-top': `solid 5px ${column.status.color}`,
													}"
												>
													<div
														v-if="activeDraggable"
														class="mr-2 flex items-center hover:cursor-move"
													>
														<EllipsisVerticalIcon
															class="h-3"
															aria-hidden="true"
														/>
														<EllipsisVerticalIcon
															class="-ml-2 h-3"
															aria-hidden="true"
														/>
													</div>

													<div
														class="group relative flex h-6 w-full items-center"
													>
													<div class="flex w-5/6 justify-between">
														<span class="text-sm">
															{{ column.title }} <span v-if="column.taskCount !== undefined">({{ column.taskCount }})</span>
														</span>
														<span class="text-sm">
															{{ column.summary }}
														</span>
													</div>

														<div
															v-tooltip.left="
																userSettings.showTooltips
																	? `Create task `
																	: { visible: false }
															"
															class="absolute right-3 top-0 opacity-0 transition duration-700 group-hover:opacity-100"
														>
															<div
																@click="openTaskModal(column)"
																class="material-icons cursor-pointer text-gray-500 hover:text-black dark:text-gray-700 dark:hover:text-white"
															>
																add
															</div>
														</div>
													</div>

													<Dropdown class="ml-auto pr-2">
														<MenuItem>
															<a
																href="#"
																class="block px-4 py-2 text-sm text-neutral-600"
																@click.prevent="openTaskModal(column)"
															>
																Create a task
															</a>
														</MenuItem>
														<MenuItem>
															<a
																href="#"
																class="block px-4 py-2 text-sm text-neutral-600"
																@click.prevent="openStatusModal(column)"
															>
																Edit status
															</a>
														</MenuItem>
														<MenuItem v-if="column.tasks.length > 0">
															<a
																href="#"
																class="block px-4 py-2 text-sm text-neutral-600"
																@click.prevent="openMoveTasksModal(column)"
															>
																Move all tasks to...
															</a>
														</MenuItem>
														<MenuItem v-if="column.status.type === 'completed'">
															<a
																href="#"
																class="block px-4 py-2 text-sm text-neutral-600"
																@click.prevent="archiveColumnTasks(column)"
															>
																Archive all
															</a>
														</MenuItem>
													</Dropdown>
												</div>
											</div>

											<div class="board-card flex-1 min-h-0 flex flex-col">
												<Draggable
													v-model="column.tasks"
													:animation="200"
													group="tasks"
													item-key="id"
													@end="onEnd"
													:disabled="false"
													:data-status="column.status.id"
													:data-column-id="column.status.id"
													class="board-card-draggable flex-1 min-h-0"
													handle=".task-drag-handle"
													:force-fallback="true"
													:touch-start-threshold="3"
												>
												<template #item="{ element: task }">
													<TaskBoardCard
														:task="task"
														:statuses="statuses"
														class="my-5"
														:data-task="jsonEncode(task)"
														@move-to-top="handleMoveToTop(task, column)"
														@move-to-bottom="handleMoveToBottom(task, column)"
														@task-deleted="loadTasks"
														@task-archived="loadTasks"
													/>
												</template>
												</Draggable>

												<Teleport v-if="tasksLoaded" :to="`.board-card-draggable[data-column-id='${column.status.id}']`">
													<div class="flex-shrink-0 px-2 mt-2">
														<div v-if="creatingTaskColumnId !== column.status.id" class="flex items-center gap-2 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 transition-colors">
															<button
																@click="startCreatingTask(column)"
																class="flex items-center gap-2 w-full py-2 px-3 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors text-sm"
															>
																<span class="material-icons text-lg">add</span>
																<span>Add task</span>
															</button>
														</div>
														<div v-else class="flex flex-col gap-2">
															<input
																v-model="newTaskTitle"
																@keyup.enter="createQuickTask(column)"
																@keyup.esc="cancelCreatingTask"
																@blur="handleInputBlur"
																placeholder="Enter task title..."
																class="task-title-input w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-tmgr-blue dark:focus:ring-tmgr-light-blue"
															/>
															<div class="flex gap-2">
																<button
																	@click="createQuickTask(column)"
																	class="flex-1 px-3 py-1.5 text-sm bg-tmgr-blue hover:bg-blue-600 text-white rounded-lg transition-colors"
																>
																	Add
																</button>
																<button
																	@click="cancelCreatingTask"
																	class="px-3 py-1.5 text-sm bg-gray-200 hover:bg-gray-300 dark:bg-gray-700 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-200 rounded-lg transition-colors"
																>
																	Cancel
																</button>
															</div>
														</div>
													</div>
												</Teleport>
											</div>
										</div>
									</div>
								</template>
							</Draggable>
						</div>
					</div>
				</div>

				<Transition name="bounce-right-fade">
					<div>
						<Modal
							v-if="isShowStatusModal"
							modal-class="p-6 w-96"
							close-on-bg-click
							@close="closeModal"
							@closing-modal="closingModal"
						>
							<template #modal-body>
								<div>
									<div v-if="!isShowColorPicker">
										<h1
											v-if="isCreatingStatus"
											class="mb-3 text-center text-xl"
										>
											Create status
										</h1>
										<h1 v-else class="mb-3 text-center text-xl">Edit status</h1>
										<label class="mb-3 flex flex-col gap-2 font-medium">
											Status name :
											<TextField
												placeholder="Name"
												v-model="statusName"
												:errors="errors.name"
											/>
										</label>
										<label class="mb-4 flex flex-col gap-2 font-medium">
											Status type :
											<Select
												placeholder="Select Type"
												:options="statusTypes"
												v-model="statusType"
												:errors="errors.type"
												label-key="name"
												value-key="name"
											/>
										</label>
										<div
											class="mb-3 mb-3 flex items-center justify-between font-medium"
										>
											<span>Status color :</span>
											<button
												type="button"
												:style="{ backgroundColor: statusColor }"
												class="w-2/4 rounded px-4 py-2 font-bold text-white outline-none transition sm:mb-0"
												:class="'bg-' + '[' + statusColor + ']'"
												@click="openPickerModal"
											>
												{{ statusColor }}
											</button>
										</div>

										<button
											@click="saveNewStatus()"
											class="mt-3 w-full rounded bg-orange-500 px-4 py-2 font-bold text-white outline-none transition hover:bg-orange-600 sm:mb-0"
											type="button"
										>
											{{ isCreatingStatus ? 'Create' : 'Save' }}
										</button>

										<button
											v-if="!isCreatingStatus"
											@click="deleteStatus"
											class="mt-3 w-full rounded bg-red-500 px-4 py-2 font-bold text-white outline-none transition hover:bg-red-700 sm:mb-0"
											type="button"
										>
											Delete
										</button>
									</div>
								</div>
							</template>
						</Modal>
						<Modal
							id="modal2"
							v-if="isShowColorPicker"
							modal-class=""
							close-on-bg-click
							@close="closePickerModal"
							@closing-modal="closingModal"
						>
							<template #modal-body>
								<div class="relative p-8">
									<button
										type="button"
										class="absolute right-1 top-1 opacity-50 transition-opacity hover:opacity-100"
									>
										<span
											class="material-icons text-2xl text-black dark:text-white"
											@click="closePickerModal"
										>
											close
										</span>
									</button>
									<color-picker
										:hue="color.hue"
										@input="onInput"
										@select="onColorSelect"
									/>
								</div>
							</template>
						</Modal>
						<Modal
							v-if="isShowMoveTasksModal"
							modal-class="p-6 w-96"
							close-on-bg-click
							@close="closeMoveTasksModal"
						>
							<template #modal-body>
								<div>
									<h1 class="mb-4 text-center text-xl font-semibold">
										Move all tasks to
									</h1>
									<p class="mb-4 text-center text-sm text-gray-600 dark:text-gray-400">
										Move {{ sourceColumnForMove?.tasks.length }} task(s) from "{{ sourceColumnForMove?.title }}" to:
									</p>
									<div class="space-y-2">
										<button
											v-for="column in availableColumnsForMove"
											:key="column.status.id"
											@click="moveAllTasksToColumn(column)"
											class="w-full rounded-lg border-2 px-4 py-3 text-left font-medium transition-all hover:bg-gray-50 dark:hover:bg-gray-800"
											:style="{ borderLeftColor: column.status.color, borderLeftWidth: '4px' }"
										>
											<div class="flex items-center justify-between">
												<span class="text-gray-900 dark:text-gray-100">{{ column.title }}</span>
												<span class="text-xs text-gray-500 dark:text-gray-400">({{ column.taskCount }})</span>
											</div>
										</button>
									</div>
									<button
										@click="closeMoveTasksModal"
										class="mt-4 w-full rounded-lg bg-gray-200 px-4 py-2 font-medium text-gray-700 transition hover:bg-gray-300 dark:bg-gray-700 dark:text-gray-200 dark:hover:bg-gray-600"
									>
										Cancel
									</button>
								</div>
							</template>
						</Modal>
					</div>
				</Transition>
			</div>

			<Transition name="fade">
				<confirm
					v-if="confirm"
					:title="confirm.title"
					:body="confirm.body"
					@onOk="confirm.action()"
					@onCancel="confirm = undefined"
				/>
			</Transition>
		</template>
	</BaseLayout>
</template>
<script>
	import Modal from '@/components/Modal.vue';
	import Button from '@/components/general/Button.vue';
	import Draggable from 'vuedraggable';

	import {
		getWorkspaceMembers,
		getWorkspaces,
		getWorkspaceStatuses,
		updateWorkspaceOrder,
	} from '@/actions/tmgr/workspaces';
	import {
		getSortedTasksByStatus,
		updateStatusOfTasks,
		updateTaskOrders,
		updateTaskStatus,
		createTask,
	} from '@/actions/tmgr/tasks';
	import { getUser, updateUser } from '@/actions/tmgr/user';
	import FiltersBoard from '@/components/general/FiltersBoard.vue';
	import Dropdown from '@/components/general/Dropdown.vue';
	import { MenuItem } from '@headlessui/vue';
	import { EllipsisVerticalIcon } from '@heroicons/vue/20/solid';
	import TaskBoardCard from '@/components/tasks/TaskBoardCard.vue';
	import TextField from '@/components/general/TextField.vue';
	import {
		createStatus,
		deleteStatus,
		updateStatus,
	} from '@/actions/tmgr/statuses';
	import ColorPicker from '@radial-color-picker/vue-color-picker';
	import Select from '@/components/general/Select.vue';
	import Confirm from '@/components/general/Confirm.vue';
	import { hslToHex, hueFromHex } from '@/utils/convertColors';
	import { getCategories } from '@/actions/tmgr/categories';
	import FilterIcon from '@/components/icons/FilterIcon.vue';
	import { BreadcrumbItem, BreadcrumbLink } from '@/components/ui/breadcrumb';
	import WorkspaceUsers from '@/components/general/WorkspaceUsers.vue';

	export default {
		name: 'Board',
		components: {
			BreadcrumbItem,
			BreadcrumbLink,
			FilterIcon,
			TaskBoardCard,
			EllipsisVerticalIcon,
			MenuItem,
			Dropdown,
			TextField,
			Modal,
			FiltersBoard,
			Button,
			Draggable,
			ColorPicker,
			Select,
			Confirm,
			WorkspaceUsers,
		},

		data: () => ({
			title: 'Board',
			user: {
				name: null,
				password: null,
				password_confirmation: null,
			},
			userData: {},
			workspacesData: [],
			workspaceId: 0,
			workspaceUsers: [],
			categories: [],
			chosenUser: null,
			chosenCategory: null,
			errors: {},
			archivedStatus: null,
			searchText: '',
			filteredTasksArray: [],
			isShowStatusModal: false,
			isCreatingStatus: false,
			statusName: '',
			statusType: '',
			statusColor: '#077fe8',
			statusId: '',
			oldColumns: [
				{
					title: 'Backlog',
					status: 'created',
					tasks: [],
				},
				{
					title: 'In Progress',
					status: 'active',
					tasks: [],
				},
				{
					title: 'Hidden',
					status: 'hidden',
					tasks: [],
				},
			],
			columns: [],
			activeDraggable: false,
			color: {
				hue: 235,
				saturation: 100,
				luminosity: 50,
				alpha: 1,
			},
			isShowColorPicker: false,
			statusTypes: [
				{ id: 1, name: 'active' },
				{ id: 2, name: 'hidden' },
				{ id: 3, name: 'archived' },
				{ id: 4, name: 'completed' },
			],
			confirm: null,
			tasks: [],
			isFiltersModalShown: false,
			hasHorizontalScroll: false,
			isMobile: window.innerWidth <= 768,
			statuses: null,
			creatingTaskColumnId: null,
			newTaskTitle: '',
			tasksLoaded: false,
			isShowMoveTasksModal: false,
			sourceColumnForMove: null,
		}),

		watch: {
			'$store.state.reloadTasksKey'() {
				this.loadTasks();
			},
			'$store.state.currentTaskIdForModal'(newVal, oldVal) {
				if (oldVal && !newVal) {
					this.$nextTick(() => {
						this.updateScrollContainers();
					});
				}
			},
			$route() {
				this.isMobile = window.innerWidth <= 768;
			},

			chosenUser: function () {
				this.loadTasks();
			},
			searchText: function () {
				this.loadTasks();
			},
			chosenCategory: function () {
				this.loadTasks();
			},
		},
		computed: {
			userSettings() {
				return this.$store.state.userSettings ?? {};
			},
			workspaceUsersWithoutAll() {
				return this.workspaceUsers.filter(user => user.id !== 0);
			},
			availableColumnsForMove() {
				if (!this.sourceColumnForMove) {
					return [];
				}
				return this.columns.filter(
					col => col.status.id !== this.sourceColumnForMove.status.id
				);
			},
		},
		methods: {
			scrollHorizontally() {
				document.querySelector('.board-container').scrollTo({
					left: document.querySelector('.board-container').scrollLeft + 200,
					behavior: 'smooth',
				});
			},
			onColorSelect() {
				const hexColor = hslToHex(
					this.color.hue,
					this.color.saturation,
					this.color.luminosity,
				);
				this.statusColor = hexColor;
				setTimeout(() => {
					this.isShowColorPicker = false;
				}, 1000);
			},
			handleFilter() {
				this.isFiltersModalShown = !this.isFiltersModalShown;
			},
			closeFilterModal() {
				this.isFiltersModalShown = false;
			},

			onInput(hue) {
				this.color.hue = hue;
			},
			handleChosenUserUpdate(newChosenUser) {
				this.chosenUser = newChosenUser;
			},
			handleChosenCategory(newChosenCategory) {
				this.chosenCategory = newChosenCategory;
			},
			openPickerModal() {
				this.isShowColorPicker = true;
				$store.commit('openModal');
			},
			closingModal() {
				if (this.isShowStatusModal && this.isShowColorPicker) {
					this.isShowColorPicker = false;
					this.$store.commit('closeModal');
					return;
				}
				if (!this.isShowColorPicker) {
					this.closeModal();
					return;
				}
			},
			closeModal() {
				this.clearStatus();
				this.isShowStatusModal = false;
				this.isShowColorPicker = false;
				this.color.hue = hueFromHex(this.statusColor);
				this.$store.commit('resetOpenModals');
			},
			closePickerModal() {
				this.isShowColorPicker = false;
				this.color.hue = hueFromHex(this.statusColor);
				this.$store.commit('closeModal');
			},
			clearStatus() {
				this.statusName = '';
				this.statusType = '';
				this.statusColor = '#077fe8';
				this.errors = {};
			},
			async saveNewStatus() {
				const newStatus = {
					name: this.statusName,
					type: this.statusType,
					color: this.statusColor,
				};

				if (this.isCreatingStatus) {
					try {
						await createStatus(this.workspaceId, newStatus);
						// this.showAlert('Save', 'The status was created');
						this.closeModal();
						await this.loadColumns();
						await this.loadTasks();
					} catch (error) {
						if (error) {
							this.errors = error.response?.data?.errors;
						}
					}
				}
				if (!this.isCreatingStatus) {
					try {
						await updateStatus(this.statusId, newStatus);
						// this.showAlert('Saved', 'The status was edited');
						this.closeModal();
						await this.loadColumns();
						await this.loadTasks();
					} catch (error) {
						if (error) {
							this.errors = error.response?.data?.errors;
						}
					}
				}
			},
			showConfirm(title, body, action) {
				this.confirm = { title, body, action };
			},
			async deleteStatus() {
				const deleteStatusConfirmation = async () => {
					await deleteStatus(this.statusId);
					this.closeModal();
					this.confirm = undefined;
					// this.showAlert('Saved', 'The status was deleted');
					await this.loadColumns();
					await this.loadTasks();
				};
				this.showConfirm(
					'Delete status',
					'Are you sure?',
					deleteStatusConfirmation,
				);
			},
			openStatusModal(column) {
				this.isShowStatusModal = true;
				this.isCreatingStatus = false;
				this.statusType = column.status.type;
				this.statusName = column.status.name;
				this.statusColor = column.status.color;
				this.statusId = column.status.id;
				this.color.hue = hueFromHex(this.statusColor);
			},
			openTaskModal(column) {
				this.$store.commit('setShowCreatingTaskModal', column.status.id);
			},
			jsonEncode(data) {
				return JSON.stringify(data);
			},
			jsonDecode(stringData) {
				return JSON.parse(stringData);
			},
			async onEnd({
				to: {
					dataset: { status },
				},
				item: {
					dataset: { task },
				},
			}) {
				task = this.jsonDecode(task);
				status = parseInt(status);

				if (task.status_id !== status) {
					await updateTaskStatus(task.id, status);
					const foundTask = this.findTask(status, task.id);
					foundTask.status_id = status;
				}
				setTimeout(() => this.saveOrders(status), 500);
			},
			async onMove() {
				const sortedStats = this.columns.map((col, index) => {
					col.status.pivot.order = index + 1;
					const { status_id, order } = col.status.pivot;

					return { status_id, order };
				});

				await updateWorkspaceOrder(this.workspaceId, {
					statuses_with_order: sortedStats,
				});
			},
			async saveOrders(status) {
				const column = this.findColumn(status);
				if (!column) {
					return;
				}
				const orders = [];
				const { tasks } = column;
				tasks.forEach(({ id }, index) => {
					orders.push({
						id: id,
						order: index + 1,
					});
				});

				await updateTaskOrders({
					tasks: orders,
				});
			},
			async handleMoveToTop(task, column) {
				const taskIndex = column.tasks.findIndex(t => t.id === task.id);
				if (taskIndex === -1 || taskIndex === 0) {
					return;
				}
				
				column.tasks.splice(taskIndex, 1);
				column.tasks.unshift(task);
				
				await this.$nextTick();
				await this.saveOrders(column.status.id);
			},
			async handleMoveToBottom(task, column) {
				const taskIndex = column.tasks.findIndex(t => t.id === task.id);
				if (taskIndex === -1 || taskIndex === column.tasks.length - 1) {
					return;
				}
				
				column.tasks.splice(taskIndex, 1);
				column.tasks.push(task);
				
				await this.$nextTick();
				await this.saveOrders(column.status.id);
			},
			findTask(status, taskId) {
				const column = this.findColumn(status);
				if (!column) {
					return null;
				}
				const { tasks } = column;
				for (let i = 0; i < tasks.length; ++i) {
					const task = tasks[i];
					if (task.id === taskId) {
						return task;
					}
				}
				return null;
			},
			async archiveColumnTasks(column) {
				if (!column) {
					return null;
				}
				const { tasks } = column;
				const taskIds = tasks.map((t) => t.id);
				const archiveStatus = this.statuses.find((s) => s.type === 'archived');
				if (!archiveStatus) {
					return;
				}
				await updateStatusOfTasks(taskIds, archiveStatus.id);
				await this.loadTasks();
			},
			openMoveTasksModal(column) {
				this.sourceColumnForMove = column;
				this.isShowMoveTasksModal = true;
				this.$store.commit('openModal');
			},
			closeMoveTasksModal() {
				this.isShowMoveTasksModal = false;
				this.sourceColumnForMove = null;
				this.$store.commit('closeModal');
			},
			async moveAllTasksToColumn(targetColumn) {
				if (!this.sourceColumnForMove || !targetColumn) {
					return;
				}
				
				const { tasks } = this.sourceColumnForMove;
				if (tasks.length === 0) {
					this.closeMoveTasksModal();
					return;
				}
				
				const taskIds = tasks.map((t) => t.id);
				const targetStatusId = targetColumn.status.id;
				const sourceColumnTitle = this.sourceColumnForMove.title;
				const targetColumnTitle = targetColumn.title;
				const taskCount = tasks.length;
				
				const confirmMove = async () => {
					try {
						await updateStatusOfTasks(taskIds, targetStatusId);
						this.confirm = undefined;
						await this.loadTasks();
					} catch (error) {
						console.error('Failed to move tasks:', error);
						this.confirm = undefined;
					}
				};
				
				this.closeMoveTasksModal();
				this.showConfirm(
					'Move all tasks',
					`Move ${taskCount} task(s) from "${sourceColumnTitle}" to "${targetColumnTitle}"?`,
					confirmMove,
				);
			},
			findColumn(status) {
				const { columns } = this;
				for (let i = 0; i < columns.length; ++i) {
					const column = columns[i];
					if (column.status.id === status) {
						return column;
					}
				}
				return null;
			},
			async loadColumns() {
				const columns = [];
				this.statuses = await getWorkspaceStatuses();
				this.statuses.sort((a, b) => a.pivot.order - b.pivot.order);

				for (let i = 0; i < this.statuses.length; ++i) {
					let status = this.statuses[i];
					if (status.type === 'archived') {
						this.archivedStatus = status;
						continue;
					}

					columns.push({
						title: status.name,
						status: status,
						tasks: [],
					});
				}

				await this.$nextTick(() => {
					this.columns = columns;
				});
			},
			handleSearchTextChanged(newValue) {
				this.searchText = newValue;
			},
			formatTime(taskTime) {
				if (taskTime < 3600) {
					let min = Math.ceil(taskTime / 60);
					return `${min} m${min > 1 ? '' : ''}`;
				} else {
					let hoursDecimal = taskTime / 3600;
					return `${hoursDecimal.toFixed(2)} h`;
				}
			},
			async loadTasks() {
				this.tasksLoaded = false;
				const tasksPromises = this.columns.map((column) =>
					this.loadTasksByStatus(column.status),
				);
				const tasksArray = await Promise.all(tasksPromises);
				if (this.searchText === '') {
					this.filteredTasksArray = tasksArray;
				} else {
					this.filteredTasksArray = tasksArray.map((el) =>
						el.filter(
							(item) =>
								item.title
									.toLowerCase()
									.includes(this.searchText.toLowerCase()) ||
								item.description
									.toLowerCase()
									.includes(this.searchText.toLowerCase()),
						),
					);
				}
				this.columns = this.columns.map((column, i) => {
					const tasksInColumn = this.filteredTasksArray[i];
					const taskCount = tasksInColumn.length;
					const summary = tasksInColumn.reduce(
						(acc, task) => task.common_time + acc,
						0,
					);
					const summaryInHours = this.formatTime(summary);

					const newColumn = { 
						...column, 
						summary: summaryInHours,
						taskCount: taskCount
					};

					newColumn.tasks = tasksInColumn;

					return newColumn;
				});
				
				this.$nextTick(() => {
					this.updateScrollContainers();
					this.tasksLoaded = true;
				});
			},
			async loadTasksByStatus(status) {
				this.tasks = await getSortedTasksByStatus(status?.id || status, {
					params: {
						'order[column]': 'order',
						'order[direction]': 'asc',
					},
				});
				if (this.$store.state.filter.selectedUser) {
					this.chosenUser = this.workspaceUsers.find(
						(user) => user.id === this.$store.state.filter.selectedUser,
					);
					this.tasks = this.tasks.filter((item) =>
						item.assignees.find(
							(assignee) =>
								assignee.id === this.chosenUser.id &&
								assignee.name === this.chosenUser.name,
						),
					);
				}
				if (this.chosenCategory?.id) {
					this.tasks = this.tasks.filter(
						(task) => task.project_category_id === this.chosenCategory.id,
					);
				}
				return this.tasks;
			},
			async saveUser() {
				try {
					this.user = await updateUser(this.user);
					this.showAlert('Saved', 'User data saved');
				} catch (e) {
					this.errors = e.response?.data?.errors || {};
				}
			},
			handleUpdateDraggable(value) {
				this.activeDraggable = value;
			},
			openCreateStatusModal() {
				this.isShowStatusModal = true;
				this.isCreatingStatus = true;
				this.clearStatus(); // Clear fields for new status
				this.$store.commit('openModal');
			},
			updateScrollContainers() {
				this.$nextTick(() => {
					const boardCards = document.querySelectorAll('.board-card');
					boardCards.forEach((card) => {
						if (card.scrollHeight > card.clientHeight) {
							card.style.overflowY = 'auto';
						}
					});
				});
			},
			startCreatingTask(column) {
				this.creatingTaskColumnId = column.status.id;
				this.newTaskTitle = '';
				this.$nextTick(() => {
					const input = document.querySelector('.task-title-input');
					if (input) {
						input.focus();
					}
				});
			},
			cancelCreatingTask() {
				this.creatingTaskColumnId = null;
				this.newTaskTitle = '';
			},
			handleInputBlur() {
				if (!this.newTaskTitle.trim()) {
					setTimeout(() => {
						if (!this.newTaskTitle.trim()) {
							this.cancelCreatingTask();
						}
					}, 200);
				}
			},
			async createQuickTask(column) {
				if (!this.newTaskTitle.trim()) {
					return;
				}

				const taskTitle = this.newTaskTitle.trim();
				this.newTaskTitle = '';
				this.creatingTaskColumnId = null;

				try {
					const user = this.$store.state.user;
					const currentWorkspaceId = this.workspaceId;

					const newTask = {
						title: taskTitle,
						description: '',
						description_json: null,
						approximately_time: 0,
						assignees: [],
						status: column.status.type || 'created',
						status_id: column.status.id,
						project_category_id: null,
						common_time: 0,
						is_daily_routine: false,
						order: 0,
						start_time: 0,
						user_id: user?.id || 0,
						workspace_id: currentWorkspaceId,
						id: undefined,
						category: 0,
						user: {
							id: user?.id || 0,
							name: user?.name || '',
						},
						checkpoints: [],
					};

					const createdTask = await createTask(newTask);
					this.$store.commit('incrementReloadTasksKey');
					await this.loadTasks();
				} catch (error) {
					console.error('Failed to create task:', error);
					this.newTaskTitle = taskTitle;
					this.creatingTaskColumnId = column.status.id;
				}
			},
		},
		async beforeMount() {
			const user = await getUser();
			this.workspacesData = await getWorkspaces();
			this.userData = user;

			const workspaceSetting = this.userData.settings.find(
				(setting) => setting.key === 'current_workspace',
			);
			if (workspaceSetting) {
				this.workspaceId = +workspaceSetting.value;
				const users = await getWorkspaceMembers(this.workspaceId);
				this.workspaceUsers = [{ id: 0, name: 'All users' }, ...users];
			}
		},
		async mounted() {
			const categoriesData = await getCategories();
			this.categories = [
				{ id: 0, title: 'All categories' },
				...categoriesData.map((cat) => ({
					id: cat.id,
					title: cat.title,
				})),
			];
			document.body.classList.add('overflow-hidden');
			await this.loadColumns();
			await this.loadTasks();
			this.color.hue = hueFromHex(this.statusColor);

			this.hasHorizontalScroll =
				document.querySelector('.board-container').scrollWidth >
				document.querySelector('.board-container').clientWidth;
			
			this.$nextTick(() => {
				this.updateScrollContainers();
			});
		},
		unmounted() {
			document.body.classList.remove('overflow-hidden');
		},
	};
</script>

<style lang="scss" scoped>
	@import '@radial-color-picker/vue-color-picker/dist/vue-color-picker.min.css';
	.settings-container {
		max-width: 700px;
		margin: 50px auto;
		padding: 20px;
		box-shadow: rgb(233 233 233) 1px 4px 20px;
	}

	.reset-scroll {
		&::-webkit-scrollbar {
			all: initial !important;
		}
		&::-webkit-scrollbar-button {
			all: initial !important;
		}
		&::-webkit-scrollbar-track {
			all: initial !important;
		}
		&::-webkit-scrollbar-track-piece {
			all: initial !important;
		}
		&::-webkit-scrollbar-thumb {
			all: initial !important;
		}
		&::-webkit-scrollbar-corner {
			all: initial !important;
		}
		&::-webkit-resizer {
			all: initial !important;
		}
	}

	.custom-scroll {
		&::-webkit-scrollbar {
			width: 1px;
		}

		&::-webkit-scrollbar-track {
			box-shadow: inset 0 0 6px rgba(0, 0, 0, 0.3);
		}

		&::-webkit-scrollbar-thumb {
			background-color: #727272;
		}
	}

	.board-container {
		display: flex;
		flex-wrap: nowrap;
		flex-grow: 1;
		overflow-x: auto;
		overflow-y: hidden;
		width: calc(100vw - 19rem);
		height: 100%;
		min-height: 0;

		@extend .reset-scroll;

		&__item {
			width: 300px;
			flex-shrink: 0;
			height: 100%;
			display: flex;
			flex-direction: column;
			min-height: 0;
		}
		@media (max-width: 768px) {
			width: calc(100vw - 3rem);
		}
	}

	.column-width {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}
	.shadow-radial::before {
		content: '';
		position: absolute;
		top: 0;
		bottom: 0;
		left: 0;
		right: 0;
		background: radial-gradient(circle, rgba(0, 0, 0, 0.5), transparent);
	}

	.board-card {
		margin-top: 20px;
		height: calc(100% - 50px);
		max-height: calc(100vh - 200px);
	}

	.board-card-draggable {
		@extend .custom-scroll;
		overflow-x: hidden;
		overflow-y: auto;

		div:first-child {
			margin-top: 0 !important;
		}
	}
</style>
