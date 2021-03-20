<template>
	<div class="w-full items-center justify-center relative">
		<transition name="bounce">
			<tasks-multiple-actions-modal
				v-if="isShowSelectedTasksCommonTime"
				:status="status"
				:is-loading-actions="isLoadingActionsForMultipleTasks"
				@updateStatus="updateStatusForSelectedTasks"
				@remove="deleteSelectedTasks"
				@export="exportSelectedTasks"
				@close="closeTimeInModal">
				<p class="text-white text-center">
					<b>Selected tasks: </b>{{ selected.filter(Boolean).length }}<br>
					{{ timeForModal }}
				</p>
			</tasks-multiple-actions-modal>
		</transition>

		<div v-selectable="hasSelectable ? { selectedGetter, selectedSetter, selectingSetter } : {}" class="relative">
			<div :class="[$color('borderSelection'), hasSelectable ? 'selection' : '']"></div>
			<div
				v-for="(task, i) in tasks"
				:key="i"
				class="w-full px-2 mt-2 selectable relative"
				:class="{'selected': !!selected[i], 'selecting': !!selecting[i], 'hover:opacity-100 opacity-50': task.deleted_at}"
				:draggable="draggable"
				:data-task-id="task.id"
				@dragstart="onDragStart($event, task)"
			>
				<bounce-loader v-if="loadingActionTasksIds.includes(task.id)" class="absolute l-0 t-0 z-10" />
				<div class="shadow-md rounded-lg md:flex" :class="{'border-solid border-l-8 border-green-600': task.start_time}">
					<div class="w-full p-4 md:p-5 flex justify-between items-center relative" :class="`${$color('blocks')} hover:${$color('blocksHover')}`">
						<task-meta
							:task="task"
							:task-time="getTaskFormattedTime(task)"
							:show-category-badges="showCategoryBadges"
						/>

						<dropdown-menu :actions="getActions(task)" />

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

	<confirm
		v-if="confirm"
		:title="confirm.title"
		:body="confirm.body"
		@onOk="confirm.action()"
		@onCancel="confirm = undefined"
	/>
</template>

<script>
	import downloadFile from "src/utils/downloadFile";
	import TasksListMixin from "src/mixins/TasksListMixin";
	import convertToQueryString from "src/utils/convertToQueryString";
	import DropdownMenu from 'components/UIElements/DropdownMenu';
	import TaskActionsInTheListMixin from "src/mixins/TaskActionsInTheListMixin";
	import TaskButtonsInTheList from "components/UIElements/Tasks/TaskButtonsInTheList";
	import TasksMultipleActionsModal from "components/UIElements/Tasks/TasksMultipleActionsModal";
	import TaskMeta from "components/UIElements/Tasks/TaskMeta";
	import Loader from "components/UIElements/Loader";
	import BounceLoader from "components/UIElements/BounceLoader";
	import Confirm from "components/UIElements/Confirm";

	export default {
		name: "TasksListComponent",
		components: {
			Confirm,
			BounceLoader,
			Loader,
			TaskMeta,
			TasksMultipleActionsModal,
			TaskButtonsInTheList,
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
			},
			hasSelectable: {
				type: Boolean,
				required: false,
				default: false
			},
			loadingActionTasksIds: {
				type: Array,
				required: false,
				default: () => []
			}
		},
		mixins: [
			TasksListMixin,
			TaskActionsInTheListMixin
		],
		data: () => ({
			confirm: null,
			isShowSelectedTasksCommonTime: false,
			selected: [],
			selecting: [],
			showTimeInModal: false,
			timeForModal: null,
			isLoadingActionsForMultipleTasks: []
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
			loadTasks() {
				this.$emit('reload-tasks')
			},
			getActions(task) {
				let actions = [
					{
						click: () => {
							this.$router.push(`/${task.id}/edit`)
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
				return [...actions, item]
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
			async updateStatusForSelectedTasks (status) {
				this.isLoadingActionsForMultipleTasks.push(status)
				for (let i = 0; i < this.tasks.length; ++i) {
					if (!this.selected[i]) {
						continue
					}
					await this.updateStatus(this.tasks[i], status, null, false)
				}
				await this.loadTasks()
				this.isLoadingActionsForMultipleTasks = this.isLoadingActionsForMultipleTasks.filter(s => s !== status)
				this.resetSelectedTasks()
			},
			showConfirm (title, body, action) {
				this.confirm = { title, body, action }
			},
			deleteSelectedTasks () {
				const deleteMultipleTasks = async () => {
					try {
						this.isLoadingActionsForMultipleTasks.push('delete')
						for (let i = 0; i < this.tasks.length; ++i) {
							if (!this.selected[i]) {
								continue
							}
							const {data: {data}} = await this.$axios.delete(`/tasks/${this.tasks[i].id}`)
							this.tasks[i].deleted_at = data.deleted_at
						}
					} catch (e) {
						console.error(e)
					} finally {
						this.confirm = null
						this.isLoadingActionsForMultipleTasks = this.isLoadingActionsForMultipleTasks.filter(s => s !== 'delete')
						this.resetSelectedTasks()
					}
				}
				this.showConfirm('Delete tasks', 'Are you sure?', deleteMultipleTasks)
			},
			capitalize(s) {
				if (typeof s !== 'string') return ''
				return s.charAt(0).toUpperCase() + s.slice(1)
			},
			selectedGetter () {
				return this.selected;
			},
			selectingSetter (arr) {
				if (arr.filter(item => item).length >= 2) {
					this.selecting = arr;
				}
			},
			selectedSetter (arr) {
				if (!this.selected.filter(Boolean).length && !this.selecting.length) {
					return
				}
				this.selected = arr.map((v, i) => this.selected[i] && v ? false : (v && !this.selected[i] ? true : (!v && this.selected[i])))

				this.isShowSelectedTasksCommonTime = this.selected.filter(Boolean).length > 1
				this.selecting = []
				if (this.isShowSelectedTasksCommonTime) {
					this.countTimeForModal()
				}
			},
			countTimeForModal () {
				const time = this.getSelectedTasks().reduce((p, c) => p + c.common_time, 0)
				this.timeForModal = this.formatTime(time)
			},
			getSelectedTasks () {
				return this.tasks.filter((task, index) => this.selected[index])
			},
			resetSelectedTasks () {
				this.isShowSelectedTasksCommonTime = false
				this.selected = []
				this.selecting = []
			},
			closeTimeInModal () {
				this.showTimeInModal = false
				this.isShowSelectedTasksCommonTime = false
				this.resetSelectedTasks()
			},
			async exportSelectedTasks (exportType = 'csv') {
				this.isLoadingActionsForMultipleTasks.push(exportType)
				const tasksIds = this.getSelectedTasks().map(({id}) => id)
				await this.defaultTasksExport(this.getExportUrl(exportType, tasksIds), exportType)
				this.isLoadingActionsForMultipleTasks = this.isLoadingActionsForMultipleTasks.filter(s => s !== status)
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
				this.isShowSelectedTasksCommonTime = this.selected.filter(Boolean).length > 1
				this.countTimeForModal()
			}
		}
	}
</script>
