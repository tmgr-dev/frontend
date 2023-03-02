<template>
	<teleport to="title">{{ title }}</teleport>

	<BaseLayout no-copyright>
		<template #body>
			<div class="block justify-center">
				<div class="w-full overflow-x-auto">
					<div class="board-container">
						<div
							v-for="column in columns"
							:key="column.title"
							class="board-container__item pr-2"
						>
							<div class="column-width h-full rounded-lg rounded px-3 py-3">
								<div>
									<div
										class="relative rounded pt-2 pl-2 pb-2 font-sans text-sm font-semibold tracking-wide text-tmgr-blue dark:text-tmgr-gray"
										:style="{
											'border-top': `solid 5px ${column.status.color}`,
										}"
									>
										{{ column.title }}
										<div class="absolute top-1/2 right-3 -translate-y-1/2">
											<dashboard-dropdown-menu :actions="getActions(column)" />
										</div>
									</div>
								</div>

								<draggable
									v-model="column.tasks"
									:animation="200"
									ghost-class="ghost-card"
									group="tasks"
									item-key="id"
									@end="onEnd"
									:data-status="column.status.id"
									class="board-card"
								>
									<template #item="{ element: task }">
										<task-card
											:task="task"
											class="my-5 cursor-move"
											:data-task="jsonEncode(task)"
										/>
									</template>
								</draggable>
							</div>
						</div>
					</div>
				</div>
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import Button from 'src/components/general/Button.vue';
	import draggable from 'vuedraggable';
	import TaskCard from 'src/components/tasks/TaskBoardCard.vue';
	import DropdownMenu from 'src/components/general/DropdownMenu.vue';
	import DashboardDropdownMenu from 'src/components/general/BoardOptionsMenu.vue';
	import { getWorkspaceStatuses } from 'src/actions/tmgr/workspaces';
	import {
		getSortedTasksByStatus,
		updateTaskOrders,
		updateTaskStatus,
	} from 'src/actions/tmgr/tasks';
	import { updateUser } from 'src/actions/tmgr/user';

	export default {
		name: 'Board',
		components: {
			DashboardDropdownMenu,
			DropdownMenu,
			Button,
			TaskCard,
			draggable,
		},
		watch: {
			'$store.getters.reloadTasks'() {
				this.loadTasks();
			},
		},
		data: () => ({
			title: 'Board',
			user: {
				name: null,
				password: null,
				password_confirmation: null,
			},
			errors: {},
			archivedStatus: null,
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
		}),
		methods: {
			getActions(column) {
				return [
					{
						click: () => {
							this.$store.commit('showCreateTaskModal', {
								showCreateTaskModal: true,
								statusId: column.status.id,
							});
						},
						label: 'Create task',
					},
				];
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
			onStart(evt) {},
			async loadColumns() {
				const columns = [];
				const statuses = await getWorkspaceStatuses();

				for (let i = 0; i < statuses.length; ++i) {
					let status = statuses[i];
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
			async loadTasks() {
				for (let i = 0; i < this.columns.length; ++i) {
					let status = this.columns[i].status;
					this.columns[i].tasks = await this.loadTasksByStatus(status);
				}
			},
			async loadTasksByStatus(status) {
				return await getSortedTasksByStatus(status?.id || status, {
					params: {
						'order[column]': 'order',
						'order[direction]': 'asc',
					},
				});
			},
			async saveUser() {
				try {
					this.user = await updateUser(this.user);
					this.showAlert('Saved', 'User data saved');
				} catch (e) {
					this.errors = e.response?.data?.errors || {};
				}
			},
		},
		async mounted() {
			document.body.classList.add('overflow-hidden');
			await this.loadColumns();
			await this.loadTasks();
		},
		unmounted(){
			document.body.classList.remove('overflow-hidden');
		}
	};
</script>

<style lang="scss" scoped>
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
		overflow-x: auto;
		overflow-y: hidden;
		@extend .reset-scroll;

		&__item {
			width: 300px;
			flex-shrink: 0;
			height: calc(100vh - 100px);
		}
	}

	.board-card {
		@extend .custom-scroll;
		overflow-x: auto;
		margin-top: 20px;
		height: calc(100% - 50px);

		div:first-child {
			margin-top: 0 !important;
		}
	}
</style>
