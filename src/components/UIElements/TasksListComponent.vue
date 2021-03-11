<template>
	<div class="w-full items-center justify-center relative">
		<transition name="bounce">
			<div v-if="showSelectedTasksCommonTime" class="draggable-block">
				<vue-draggable-resizable :resizable="false" :w="250" class-name="rounded bg-green-600 py-5 px-2">
					<p class="text-white text-center">
						<b>Selected tasks: </b>{{ selected.filter(v => v).length }}<br>
						{{ timeForModal }}
					</p>
					<div class="w-full text-center">
						<a href="#close"
							 class="absolute top-0 right-0 pr-1 pt-1 text-gray-400 hover:text-gray-100"
							 @click.prevent="closeTimeInModal">
							<span class="material-icons text-bold">close</span>
						</a>
						<button
							@click="selectedUpdateStatus('done')"
							class="mr-1 w-1/5 bg-green-700 text-white rounded py-2 mt-2 hover:bg-green-600">
						<span class="relative">
							<span class="material-icons text-bold">done</span>
						</span>
						</button>
						<button
							@click="selectedUpdateStatus('active')"
							class="mr-1 w-1/5 bg-purple-700 text-white rounded py-2 mt-2 hover:bg-purple-600">
						<span class="relative">
							<span class="material-icons text-bold">refresh</span>
						</span>
						</button>
						<button
							@click="selectedUpdateStatus('hidden')"
							class="mr-1 w-1/5 bg-green-700 text-white rounded py-2 mt-2 hover:bg-green-600">
							<span class="relative">
								<span class="material-icons">visibility_off</span>
							</span>
						</button>
						<button
							v-if="status === 'hidden' || status === 'done'"
							@click="deleteSelectedTasks()"
							class="w-1/5 bg-red-700 text-white rounded py-2 mt-2 hover:bg-red-600">
							<span class="relative">
								<span class="material-icons">delete</span>
							</span>
						</button>
						<button
							@click="exportSelectedTasksTo()"
							class="w-1/5 bg-blue-700 text-white rounded py-2 mt-2 hover:bg-blue-600">
							<span class="relative">
								<span class="material-icons">description</span>
							</span>
						</button>
						<button
							@click="exportSelectedTasksTo('jpg')"
							class="w-1/5 bg-blue-700 ml-1 text-white rounded py-2 mt-2 hover:bg-blue-600">
							<span class="relative">
								<span class="material-icons">image</span>
							</span>
						</button>
						<button
							@click="exportSelectedTasksTo('xlsx')"
							class="w-1/5 bg-blue-700 text-white ml-1 rounded py-2 mt-2 hover:bg-blue-600">
							<span class="relative">
								<span class="material-icons">list</span>
							</span>
						</button>
					</div>
				</vue-draggable-resizable>
			</div>
		</transition>

		<div v-selectable="{ selectedGetter, selectedSetter, selectingSetter }" class="relative">
			<div class="selection" :class="$color('borderSelection')"></div>
			<div
				v-for="(task, i) in tasks"
				:key="i"
				class="w-full px-2 mt-2 selectable"
				:class="{'selected': !!selected[i], 'selecting': !!selecting[i], 'hover:opacity-100 opacity-50': task.deleted_at}"
				:draggable="draggable"
				:data-task-id="task.id"
				@dragstart="onDragStart($event, task)"
				@dragend="onDragEnd($event, task)"
			>
				<div class="shadow-xl rounded-lg md:flex" :class="{'border-solid border-l-8 border-green-600': task.start_time}">
					<div class="w-full">
						<div class="p-4 md:p-5" :class="`${$color('blocks')} hover:${$color('blocksHover')}`">
							<div class="flex justify-between items-center relative">
								<div>
									<div class="flex">
										<router-link :to="`/${task.id}/edit`" class="font-bold text-xl z-10">
											{{ task.title }}
										</router-link>
										<div class="flex items-start task-category-in-task">
											<router-link
												v-if="task.category && showCategoryBadges"
												:to="{name: 'ProjectCategoryChildrenList', params: {id: task.category.id}}"
												class="inline bg-gray-700 text-white py-1 px-2 rounded ml-2 leading-none text-base z-10"
											>
												{{ task.category.title }}
											</router-link>
											<router-link
												:to="`/${task.category ? 'project-categories/' + task.category.id + '/' : ''}tasks/create`" title="Add task to category"
												class="opacity-25 hover:opacity-100 tc-hidden md:inline add-task-to-category-from-task-category z-10">
												<span class="material-icons text-3xl -mt-1">add_circle_outline</span>
											</router-link>
										</div>
									</div>
									<div class="flex items-start">
										<span>
											<span class="material-icons text-xl" :class="task.start_time ? 'text-green-600' : 'text-orange-600'">alarm</span>
										</span>
										<span class="text-gray-700 ml-2">{{ getTaskFormattedTime(task) }}</span>
									</div>
								</div>

								<dropdown-menu class="lg:hidden" :actions="getActions(task)" />

								<task-buttons-in-the-list
									:task="task"
									:showed-buttons="getShowButtons(task)"
									:is-loading-actions="isLoadingActions"
									@updateStatus="updateStatus"
									@stopCountdown="stopCountdown"
									@startCountdown="startCountdown"
								/>
							</div>
						</div>
					</div>
				</div>
			</div>
		</div>
	</div>
</template>

<script>
	import downloadFile from "src/utils/downloadFile";
	import TasksListMixin from "src/mixins/TasksListMixin";
	import convertToQueryString from "src/utils/convertToQueryString";
	import DropdownMenu from 'src/components/UIElements/DropdownMenu';
	import TaskActionsInTheListMixin from "src/mixins/TaskActionsInTheListMixin";
	import TaskButtonsInTheList from "src/components/UIElements/Task/TaskButtonsInTheList";
	import VueDraggableResizable from "src/components/UIElements/VueDraggableResizable/VueDraggableResizable";

	export default {
		name: "TasksListComponent",
		components: {
			TaskButtonsInTheList,
			VueDraggableResizable,
			DropdownMenu
		},
		emits: ['reload-tasks'],
		props: {
			tasks: {
				required: false,
				type: Array,
				default: () => []
			},
			status: {
				required: false,
				type: String,
				default: null
			},
			useTaskStatusForButtons: {
				required: false,
				type: Boolean,
				default: false
			},
			showCategoryBadges: {
				required: false,
				type: Boolean,
				default: true
			},
			isLoadingActions: {
				required: true,
				type: Object
			},
			draggable: {
				type: Boolean,
				required: false,
				default: false
			}
		},
		mixins: [
			TasksListMixin,
			TaskActionsInTheListMixin
		],
		data: () => ({
			showSelectedTasksCommonTime: false,
			selected: [],
			selecting: [],
			showTimeInModal: false,
			timeForModal: null,
		}),
		methods: {
			async stopCountdown(task, dotId) {
				this.isLoadingActions[dotId] = true
				await this.$axios.delete(`tasks/${task.id}/countdown`)
				await this.loadTasks()
				this.isLoadingActions[dotId] = false
			},
			async startCountdown(task, dotId) {
				this.isLoadingActions[dotId] = true
				await this.$axios.post(`tasks/${task.id}/countdown`)
				await this.loadTasks()
				this.isLoadingActions[dotId] = false
			},
			onDragStart (event, task) {
				event.dataTransfer.setData('task-id', task.id)
			},
			onDragEnd (event, task) {},
			async loadTasks() {
				await this.$emit('reload-tasks')
			},
			getActions(task) {
				let actions = [
					{
						click: () => {
							this.$router.push(`/tasks/${task.id}/edit`)
						},
						label: 'Edit'
					}
				]

				actions = this.addActionItem(actions, this.getActionItem(task, 'active', 'Activate'), this.getShowButtons(task).activate)
				actions = this.addActionItem(actions, this.getActionItem(task, 'hidden', 'Hide'), this.getShowButtons(task).hide)
				actions = this.addActionItem(actions, this.getActionItem(task, 'done', 'Done'), this.getShowButtons(task).done)

				return actions
			},
			getShowButtons(task) {
				return {
					done: (this.status !== 'done' && !this.useTaskStatusForButtons) || (this.useTaskStatusForButtons && task.status !== 'done'),
					activate: (this.status === 'done' || this.status === 'hidden' && !this.useTaskStatusForButtons) || (this.useTaskStatusForButtons && (task.status === 'done' || task.status === 'hidden')),
					hide: (this.status !== 'hidden' && this.status !== 'done' && !this.useTaskStatusForButtons) || (this.useTaskStatusForButtons && (task.status !== 'hidden' && task.status !== 'done')),
					start: task.start_time,
					stop: !task.start_time && (task.status === 'active' || task.status === 'created'),
					deleteTask: this.status === 'hidden' || this.status === 'done'
				}
			},
			addActionItem(actions, item, show = true) {
				if (!item || !show) {
					return actions
				}
				actions.push(item)
				return actions
			},
			getActionItem(task, status, label) {
				if (status === this.status) {
					return null
				}
				return {
					click: () => {
						this.updateStatus(task, status)
					},
					label: label
				}
			},
			async updateStatus(task, status, dotId = null, loadTasks = true) {
				try {
					this.setLoadingAction(dotId)
					await this.$axios.put(`/tasks/${task.id}/${status}`)
					if (loadTasks) {
						await this.loadTasks()
					}
				} catch (e) {
					console.error(e)
				} finally {
					this.setLoadingAction(dotId, false)
				}
			},
			async selectedUpdateStatus(status) {
				for (let i = 0; i < this.tasks.length; ++i) {
					if (!this.selected[i]) {
						continue
					}
					await this.updateStatus(this.tasks[i], status, null, false)
				}
				await this.loadTasks()
				this.resetSelectedTasks()
			},
			async deleteSelectedTasks () {
				for (let i = 0; i < this.tasks.length; ++i) {
					if (!this.selected[i]) {
						continue
					}
					await this.deleteTask(this.tasks[i], null, false)
				}
				// await this.loadTasks()
				this.resetSelectedTasks()
			},
			capitalize(s) {
				if (typeof s !== 'string') return ''
				return s.charAt(0).toUpperCase() + s.slice(1)
			},
			selectedGetter () {
				return this.selected;
			},
			selectedSetter (arr) {
				this.selected = arr.map((v, i) => this.selected[i] && v ? false : (v && !this.selected[i] ? true : (!v && this.selected[i])))

				this.showSelectedTasksCommonTime = this.selected.filter(Boolean).length > 1
				this.selecting = []
				this.countTimeForModal()
			},
			countTimeForModal () {
				const time = this.getSelectedTasks().reduce((p, c) => p + c.common_time, 0)
				this.timeForModal = this.formatTime(time)
			},
			getSelectedTasks () {
				return this.tasks.filter((task, index) => this.selected[index])
			},
			resetSelectedTasks () {
				this.selected = []
				this.selecting = []
			},
			closeTimeInModal () {
				this.showTimeInModal = false
				this.showSelectedTasksCommonTime = false
				this.resetSelectedTasks()
			},
			selectingSetter (arr) {
				if (arr.filter(item => item).length >= 2) {
					this.selecting = arr;
				}
			},
			async exportSelectedTasksTo(exportType = 'csv') {
				const tasksIds = this.getSelectedTasks().map(({id}) => id)
				await this.defaultTasksExport(this.getExportUrl(exportType, tasksIds), exportType)
			},
			async defaultTasksExport (url, exportType = 'csv') {
				const response = await this.$axios.get(url, {
					responseType: 'blob'
				})
				downloadFile(response.data, 'export.' + exportType)
			},
			getExportUrl(exportType, tasksIds) {
				return 'exports/tasks/' + exportType + '?' + convertToQueryString({ ids: tasksIds, per_hour: 1000 })
			},
			selectAll () {
				this.selected = this.tasks.map(() => true)
				this.showSelectedTasksCommonTime = this.selected.filter(Boolean).length > 1
				this.countTimeForModal()
			}
		}
	}
</script>
