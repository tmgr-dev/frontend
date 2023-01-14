<template>
	<teleport to="title">
		{{ form.title || h1.main }}&nbsp;
	</teleport>
	<BaseLayout>
		<template #header>
			<span>{{ form.title || h1.main }}</span>
		</template>
		<template #action>
			<div class="sm:flex items-between text-center">
				<router-link
					v-if="!isCreatingTask"
					:to="!currentCategory ? '/' : `/projects-categories/${currentCategory.id}/children/${getCategoryStatus()}`"
					class="py-2 rounded focus:outline-none sm:mb-0 mb-5"
					:class="`${$color('h1')}-800`"
					type="button">
					&lt; {{ currentCategory ? currentCategory.title : 'Tasks' }}
				</router-link>

				<div ref="editing_task_category" v-if="!isCreatingTask">
					<a href="#" class="absolute left-0 top-0 text-gray-600 p-5 pt-4 md:p-0" :class="$color('grayHover')" @click.prevent="showModalCategory">
						<span class="material-icons text-lg">settings</span>
					</a>

					<modal
						v-if="isShowModalCategory"
						:modal-width="500"
						:is-center="true"
						:close-on-bg-click="false"
						@close="isShowModalCategory = false">
						<template #modal-body>
							<form @submit.prevent="updateCategory">
								<div>
									<label :class="`block text-sm text-left font-bold bg-gray-400 mb-2 mt-2 text-black ${$color('taskSettingTextColor')}`" for="">
										Category
										<vue-select
											label="title"
											:options="categoriesSelectOptions"
											v-model="currentCategoryOptionInSelect"
										/>
									</label>
								</div>
								<div>
									<label :class="`block text-sm text-left font-bold bg-gray-400 mb-2 mt-2 text-black ${$color('taskSettingTextColor')}`" for="">
										Estimated time
										<input-field extra-class="bg-gray-400" v-model="form.approximately_time" :errors="errors.approximately_time" type="time_in_seconds" placeholder="Enter approximately time"/>
									</label>
								</div>

								<hr class="py-2">

								<label for="settings" class="tc-block text-gray-700 text-sm font-bold mb-5">
									Settings
								</label>
								<div>
									<div v-for="(setting, index) in availableSettings" id="settings">
										<label :for="`setting-${setting.id}`" class="tc-block text-gray-700 text-sm font-bold mb-2">
											{{ setting.name }}
										</label>
										<div class="relative mb-4">
											<select :id="`setting-${setting.id}`"
															v-if="!setting.show_custom_value_input"
															class="tc-block appearance-none w-full bg-white border border-gray-300 px-4 py-2 pr-8 rounded shadow leading-tight focus:outline-none focus:shadow-outline"
															v-model="settings[index].value">
												<option value="" class="text-gray-500">Choose default value</option>
												<option v-for="(c, i) in setting.default_values" :key="i" :value="c.value">
													{{ c.value }}
												</option>
											</select>
											<div
												v-else-if="setting.custom_value_available"
											>
												<input-field
													:extra-class="`bg-gray-400  ${$color('taskSettingTextColor')}`"
													:id="`setting-${setting.id}`"
													:type="setting.component_type"
													:placeholder="setting.description"
													v-model="settings[index].value"
													:tag="settings[index].id = setting.id"
												/>
											</div>
											<small v-if="!setting.show_custom_value_input">{{ setting.description }}</small>
											<div class="b-switch-list" v-if="setting.custom_value_available">
												<div
													class="b-switch-list__item"
													v-if="setting.default_values && setting.default_values.length > 0"
												>
													<label class="b-switch">
														<input type="checkbox" name="show_tooltips" v-model="setting.show_custom_value_input" @change="settings[index].value = ''">
														<span></span>
													</label>
													<div
														class="b-switch-list__text"
													>
														<div class="b-switch-list__title" :class="$color('settingsTextColor')">Set custom value</div>
													</div>
												</div>
											</div>
										</div>
									</div>
								</div>

								<div class="flex flex-nowrap items-center mt-5">
									<button
										type="button"
										@click="isShowModalCategory = false"
										class="tc-block w-2/4 mr-1 bg-gray-700 text-white p-2 rounded">
										Cancel
									</button>
									<button
										type="submit"
										class="tc-block w-2/4 mr-1 bg-blue-700 text-white p-2 rounded">
										Update
									</button>
								</div>
							</form>
						</template>
					</modal>
				</div>

				<div v-if="!isCreatingTask" class="text-base sm:text-lg ml-auto">
					<button
						v-for="status in workspaceStatuses"
						type="button"
						@click="form.status_id = status.id"
						:class="form.status_id !== status.id ? `opacity-25 hover:opacity-100` : ``"
						class="inline sm:mr-2 sm:ml-2 mr-1 text-white p-2 rounded leading-none"
						:style="`background-color: ${status.color}`"
					>
						{{ status.name }}
					</button>
				</div>
			</div>
		</template>
		<template #body>
			<div :class="`${$color('blocks')} rounded-lg relative`">
				<div class="lg:flex">
					<div class="lg:w-full h-full mt-5 p-5">
						<div :class="`bg-white pl-5 pr-5 h-full ${$color('blocks')}`">
							<div class="tc-block w-full float-left">
								<div class="mb-2">
									<input-field v-model="form.title" :errors="errors.title" type="text" placeholder="Enter task title"/>
								</div>
							</div>
						</div>
					</div>
				</div>

				<div class="lg:flex px-5">
					<div class="w-full h-full">
						<div :class="`${$color('blocks')}  px-5 pb-1`">
							<div class="mb-2">
								<label class="tc-block text-sm text-left font-bold mb-2" for="">
									Description
									<a
										v-if="showEditDescription && !isCreatingTask"
										href="#"
										class="text-red-600 relative"
										style="top:4px"
										@click.prevent="showEditDescription = false">
										<span class="material-icons text-base">cancel</span>
									</a>
								</label>
								<input-field
									v-if="showEditDescription || isCreatingTask"
									v-model="form.description"
									:errors="errors.description"
									type="textarea"
									placeholder="Description"
								/>
								<p
									class="text-left whitespace-pre-line"
									v-else>
									<a href="#" class="text-blue-600 relative" style="top:2px" @click.prevent="showEditDescription = true">
										<span class="material-icons text-base">edit</span>
									</a>
									{{ form.description || 'Enter your task description'  }}
								</p>
							</div>
						</div>
					</div>
				</div>
				<div class="lg:flex">
					<div class="w-full h-full mt-5">
						<div :class="`${$color('blocks')} p-5 h-full`">
							<Countdown
								v-if="form.id"
								:init-task="form"
								@toggle="toggleCountdown"
								@update:seconds="updateSeconds"
							/>
							<Countdown
								v-else
								disabled
								:init-task="form"
								@update:seconds="updateSeconds"
							/>
						</div>
					</div>
				</div>
				<div class="w-full p-5">
					<h2 class="text-2xl pt-5" v-if="!isCreatingTask && form.checkpoints && form.checkpoints.length">
						Checkpoints
					</h2>
					<div v-if="!isCreatingTask" :class="`${$color('blocks')} rounded mt-5 p-5`" :key="checkpointUpdateKey">
						<div class="mb-5" v-for="(checkpoint, v) in form.checkpoints" :key="v">
							<div class="flex mb-2">
								<div class="w-full mx-2 relative">
									<span
										:class="`shadow ${$color('borderMain')} ${$color('input')} appearance-none border rounded w-full py-2 px-3  leading-tight focus:outline-none focus:shadow-outline ${errors ? 'with-errors' : ''}`"
									>
										{{ v + 1 }}
									</span>
									<span
										:class="`shadow ${$color('borderMain')} ${$color('input')} appearance-none border rounded w-full py-2 px-3  leading-tight focus:outline-none focus:shadow-outline ${errors ? 'with-errors' : ''}`"
									>
										{{ secondsToStringTime(checkpoint.start) }}
									</span>
									<span
										:class="`shadow ${$color('borderMain')} ${$color('input')} appearance-none border rounded w-full py-2 px-3  leading-tight focus:outline-none focus:shadow-outline ${errors ? 'with-errors' : ''}`"
									>
										{{ secondsToStringTime(checkpoint.end) }}
									</span>
									<input-field
										type="text"
										placeholder="Checkpoint content"
										v-model="checkpoint.description"
									/>
									<span class="absolute right-0 top-0 mt-5 checkpoint-delete" @click="deleteCheckpoint(v)">
									<span class="material-icons text-lg text-red-700">delete</span>
								</span>
								</div>
							</div>
						</div>
					</div>

					<task-actions
						:is-creating-task="isCreatingTask"
						:is-data-edited="isDataEdited"
						:is-saving="isSaving"
						@addCheckpoint="addCheckpoint"
						@createTask="createTask"
						@saveTask="saveTask"
					/>

				</div>
				<p v-if="form.approximately_time" class="text-gray-500 pl-4 pb-2">
					Estimated time to complete the task: {{ toHHMM(form.approximately_time) }}
				</p>
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import moment from 'moment'
	import InputField from "src/components/UIElements/InputField";
	import TaskActions from "src/components/UIElements/Tasks/TaskActions";

	export default {
		name: 'TasksForm',
		components: {
			TaskActions,
			InputField
		},
		data() {
			return {
				savedData: {},
				availableSettings: [],
				settings: [],
				isSaving: false,
				autoSaveTimeout: null,
				watchingFields: [
					'title',
					'description',
					'status',
					'project_category_id',
					'checkpoints',
					'approximately_time',
					'settings'
				],
				errors: {},
				showEditDescription: false,
				panel: false,
				isOpen: false,
				checkpointUpdateKey: 0,
				counter: {
					common_time: 0,
					start_time: 0
				},
				statuses: [
					{value: 'created', name: `Created`},
					{value: 'active', name: `Active`},
					{value: 'done', name: `Done`}
				],
				h1: {
					main: `${this.taskId ? 'Edit' : 'Create'} task`,
					CurrentTasksList: 'Current tasks',
					HiddenTasksList: 'Hidden tasks',
					ArchiveTasksList: 'Archive tasks'
				},
				selected: false,
				form: {
					title: '',
					status: 'created',
					project_category_id: this.projectCategoryId || '',
					description: '',
					common_time: 0,
					checkpoints: []
				},
				isShowModalCategory: false,
				categoriesSelectOptions: [],
				currentCategory: '',
				approximatelyTime: null,
				currentCategoryOptionInSelect: null,
				prevValue: null
			}
		},
		watch: {
			form (newVal) {
				this.setSavedData(newVal)
			}
		},
		computed: {
			taskId () {
				return this.$route.params.id
			},
			workspaceStatuses () {
				return this.$store.getters.statuses
			},
			isCreatingTask() {
				return !this.taskId
			},
			projectCategoryId() {
				return this.$route.params.project_category_id
			},
			isDataEdited () {
				if (!this.form.id) {
					return false
				}
				for(let i = 0; i < this.watchingFields.length; ++i) {
					const field = this.watchingFields[i]

					if (field === 'checkpoints') {
						const first = this.removeFieldsFromArray(this.savedData[field], ['end'])
						const second = this.removeFieldsFromArray(this.form[field], ['end'])
						if (!this.equals(first, second)) {
							if (this.equals(this.prevValue, second)) {
								return true
							}
							this.prevValue = JSON.parse(JSON.stringify(second))
							this.dispatchAutoSave()
							return true
						}
					} else if (!this.equals(this.savedData[field], this.form[field])) {
						this.dispatchAutoSave()
						return true
					}
				}
				return false
			}
		},
		methods: {
			async loadTaskSettings() {
				const {data: {data}} = await this.$axios.get('tasks/settings')
				this.initSettings(data, this.form.settings)
				this.availableSettings = data
			},
			initSettings(availableSettings, settings = []) {
				return availableSettings.map((item, index) => {
					const setting = this.getSettingById(settings, item.id, {
						id: item.id,
						value: ''
					})
					this.settings[index] = setting
					item.show_custom_value_input = item.default_values && item.default_values.findIndex(val => val.value === setting.value) === -1
				})
			},
			getSettingById(settings, id, defaultResult = null) {
				return settings.find(setting => setting.id === id) || defaultResult
			},
			async saveSettings(settings) {
				const {data: {data}} = await this.$axios.put(`/tasks/${this.form.id}/settings`, settings)
				this.initSettings(this.availableSettings, data.settings)
			},
			setFormDataWithDelay(data, delay = 200) {
				return new Promise((resolve) => {
					if (this.form.id === data.id) {
						this.form.id = null
						setTimeout(() => {
							this.form = data
							resolve()
						}, delay)
					}
				})
			},
			dispatchAutoSave() {
				this.removeDispatchedAutoSave()
				this.autoSaveTimeout = setTimeout(this.saveTask, 5000)
			},
			removeDispatchedAutoSave() {
				if (this.autoSaveTimeout) {
					clearTimeout(this.autoSaveTimeout)
				}
			},
			equals (o1, o2) {
				return JSON.stringify(o1) === JSON.stringify(o2)
			},
			removeFieldsFromArray (arr, fieldsForDelete) {
				if (!arr) {
					return
				}
				arr = JSON.parse(JSON.stringify(arr))
				return arr.map(item => {
					const keys = Object.keys(item).filter(key => !fieldsForDelete.includes(key))
					const result = {}
					keys.forEach(key => result[key] = item[key])
					return result
				})
			},
			async showModalCategory () {
				try {
					this.isShowModalCategory = true
					if (this.categoriesSelectOptions.length === 0) {
						await this.loadCategories()
					}
				} catch (e) {
					console.error(e)
				}
			},
			async updateCategory () {
				if (this.currentCategoryOptionInSelect) {
					this.form.project_category_id = this.currentCategoryOptionInSelect.id
				}
				this.isShowModalCategory = false
				await this.saveTask()
				await this.loadCategory()
			},
			getShortcutSaveListener() {
				return (e) => {
					if (e.ctrlKey && (e.key.toLowerCase() === 's' || e.key.toLowerCase() === 'ы')) {
						e.preventDefault();
						this.saveTask()
					}
				}
			},
			getActions() {
				const actions = [
					{
						click: () => {
							this[this.form.id ? 'save' : 'create']()
						},
						label: this.form.id ? 'Save' : 'Create'
					}
				]
				actions.push({
					click: () => {
						this[this.isCreatingTask ? 'cancel' : 'goToCurrentTasks']()
					},
					label: this.isCreatingTask ? 'Cancel' : 'Tasks'
				})
				return actions
			},
			async loadCategories() {
				const {data: {data}} = await this.$axios.get('project_categories?all')
				this.categoriesSelectOptions = data
			},
			async loadCategory() {
				if (this.projectCategoryId || this.form.project_category_id) {
					const {data: {data}} = await this.$axios.get(`project_categories/${this.projectCategoryId || this.form.project_category_id}`)
					this.currentCategory = data
					this.currentCategoryOptionInSelect = data

					if (!!this.form.id || this.currentCategory.settings.length === 0) {
						return
					}

					this.currentCategory.settings.forEach(setting => {
						if (setting.key === 'task_name_pattern_date&time') {
							this.form.title = moment().format(setting.value)
						}
						if (setting.key === 'approximately_time') {
							this.form.approximately_time = parseInt(setting.value)
						}
					})
				}
			},
			async loadModel() {
				const {data: {data}} = await this.$axios.get(`tasks/${this.taskId}`)
				data.common_time = data.common_time || 0
				this.form = data

				this.$store.commit('currentOpenedTaskId', this.form.id)
			},
			setSavedData(data) {
				this.watchingFields.forEach(field => this.savedData[field] = JSON.parse(JSON.stringify(data[field])))
			},
			toHHMM (seconds) {
				let hours   = Math.floor(seconds / 3600);
				let minutes = Math.floor((seconds - (hours * 3600)) / 60);

				if (hours   < 10) {
					hours   = "0" + hours;
				}
				if (minutes < 10) {
					minutes = "0" + minutes;
				}
				return hours + ':' + minutes;
			},
			getCategoryStatus() {
				if (this.form.status === 'created' || this.form.status === 'active') {
					return 'current'
				}
				return this.form.status
			},
			async toggleCountdown() {
				this.form.id = null
				const { data: {data} } = await this.$axios[this.form.start_time ? 'delete' : 'post'](`tasks/${this.taskId}/countdown`)
				this.form = { ...data }
				this.updateSeconds(this.form.common_time)
			},
			prepareForm() {
				if (this.form.project_category_id === '') {
					delete this.form.project_category_id
				}
			},
			async createTask () {
				try {
					this.prepareForm()
					const {data: {data}} = await this.$axios.post('tasks', this.form)
					if (!this.isCreatingTask) {
						this.showAlert()
					}
					await this.$router.push({
						name: 'TasksEdit',
						params: {
							id: data.id
						}
					})
				} catch (e) {
					if (e.response && e.response && e.response.data.errors) {
						this.errors = e.response.data.errors
					}
				}
			},
			async saveTask () {
				try {
					this.isSaving = true
					this.prepareForm()
					const {data: {data}} = await this.$axios.put(`tasks/${this.taskId}`, this.form)
					if (data.approximately_time) {
						this.approximatelyTime = this.toHHMM(data.approximately_time)
					}
					this.form = data

					await this.saveSettings(this.settings)

					this.showAlert('Saved', 'The task was saved')
					this.removeDispatchedAutoSave()
					const id = this.form.id
					this.form.id = null
					setTimeout(() => this.form.id = id, 1)
				} catch (e) {
					if (e.response && e.response && e.response.data.errors) {
						this.errors = e.response.data.errors
					}
				} finally {
					this.isSaving = false
				}

			},
			goToCurrentTasks() {
				this.$router.push('/')
			},
			secondsToStringTime(seconds) {
				const second = seconds % 60
				let minute = (seconds - second) / 60 | 0
				const hour = minute / 60 | 0
				minute = minute - (hour * 60)

				return `${this.prepareClockNumber(hour)}:${this.prepareClockNumber(minute)}:${this.prepareClockNumber(second)}`
			},
			stringTimeToSeconds(stringTime) {
				let times = stringTime.split(':')
				times = times.map(parseInt)
				return times[0] * 60 * 60 + times[1] * 60 + times[2]
			},
			prepareClockNumber(num) {
				return num < 10 ? '0' + num : num
			},
			addCheckpoint() {
				const { form } = this
				if (!form.checkpoints) {
					form.checkpoints = []
				}
				const timeSinceStartTime = !form.start_time ? 0 : Math.floor(
					((new Date()) - (new Date()).setTime(form.start_time * 1000)) / 1000
				)
				const currentTime = form.common_time + timeSinceStartTime
				if (form.checkpoints.length > 0) {
					const prevCheckpointIndex = form.checkpoints.length - 1
					form.checkpoints[prevCheckpointIndex].end = currentTime
				}
				form.checkpoints.push({
					description: 'New one',
					start: currentTime,
					end: currentTime
				})
				++this.checkpointUpdateKey
			},
			deleteCheckpoint(checkpointIndex) {
				this.form.checkpoints.splice(checkpointIndex, 1)
				++this.checkpointUpdateKey
			},
			updateSeconds(seconds) {
				if (!this.form.checkpoints || this.form.checkpoints.length === 0) {
					return
				}
				this.form.checkpoints[this.form.checkpoints.length - 1].end = seconds
			}
		},
		async created () {
			if (this.taskId) {
				await this.loadModel()
				window.onkeydown = this.getShortcutSaveListener()
			}

			if (this.projectCategoryId && this.isCreatingTask) {
				this.form.project_category_id = this.projectCategoryId
			}
			await this.loadCategory()
			await this.loadTaskSettings()
			this.$store.getters.pusher.private(`App.User.${this.$store.getters.user.id}`)
				.on('task-countdown-stopped', ({task}) => {
					const isCountdownStarted = !!this.form.start_time
					if (!isCountdownStarted) {
						return
					}

					this.setFormDataWithDelay(task).then(() => {
						this.showAlert('Countdown stopped')
					})
				})
				.on('task-countdown-started', ({task}) => {
					const isCountdownStarted = !!this.form.start_time
					if (!isCountdownStarted) {
						this.setFormDataWithDelay(task).then(() => {
							this.showAlert('Countdown started')
						})
					}
				})
		},
	}
</script>

<style lang="scss" scoped>
	.checkpoint-delete {
		top: 9px;
		right: 10px;
		cursor: pointer;
		opacity: 0.5;
		&:hover {
			opacity: 1;
		}
	}

	.checkpoint-index {
		width: fit-content;
		top: 7px;
		left: 10px;
		cursor: pointer;
		color: #00c300;
		opacity: 0.5;
		&:hover {
			opacity: 1;
		}
	}

	.task-title-span {
		max-width: 200px;
		display: inline-block;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		top: 16px;
		position: relative;
	}
</style>
