<template>
	<teleport to="title">
		{{ form.title || h1.main }}
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

								<div class="flex items-center mt-5">
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
					<button type="button" @click="form.status = 'created'" :class="form.status !== `created` ? `opacity-25 hover:opacity-100` : ``" class="inline sm:mr-2 sm:ml-2 mr-1 bg-blue-700 text-white p-2 rounded leading-none">
						Created
					</button>
					<button type="button" @click="form.status = 'active'" :class="form.status !== `active` ? `opacity-25 hover:opacity-100` : ``" class="inline sm:mr-2 sm:ml-2 mr-1 bg-blue-700 text-white p-2 rounded leading-none">
						Active
					</button>
					<button type="button" @click="form.status = 'hidden'" :class="form.status !== `hidden` ? `opacity-25 hover:opacity-100` : ``" class="inline sm:mr-2 sm:ml-2 mr-1 bg-gray-700 text-white p-2 rounded leading-none">
						Hidden
					</button>
					<button type="button" @click="form.status = 'done'" :class="form.status !== `done` ? `opacity-25 hover:opacity-100` : ``" class="inline sm:ml-2 mr-1 bg-red-700 text-white p-2 rounded leading-none">
						Done
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
								ref="countdown"
								:init-task="form"
								@toggle="toggleCountdown"
								@update:seconds="updateSeconds"
							/>
							<Countdown
								v-else
								ref="countdown"
								:init-task="form"
								:style="{
									opacity: 0.2,
									'pointer-events': 'none'
								}"
							/>
						</div>
					</div>
				</div>
				<div class="w-full p-5">
					<h2 class="text-2xl pt-5" v-if="!isCreatingTask && form.checkpoints && form.checkpoints.length">
						Checkpoints
					</h2>
					<div v-if="!isCreatingTask" :class="`${$color('blocks')} rounded mt-5 p-5`" :key="checkpointUpdateKey">
						<div class="mb-2" v-for="(checkpoint, v) in form.checkpoints" :key="v">
							<div class="flex mb-2">
								<div class="w-full mx-2 relative">
									<span class="absolute right-0 top-0 checkpoint-index">{{ v + 1 }}</span>
									<input
										:class="`pl-10 shadow appearance-none border rounded w-full py-2 px-3 ${$color('input')} ${$color('borderMain')}  leading-tight focus:outline-none focus:shadow-outline `"
										type="text"
										placeholder="Checkpoint content"
										v-model="checkpoint.description"
									>
									<span class="absolute right-0 top-0 checkpoint-delete" @click="deleteCheckpoint(v)">
									<span class="material-icons text-lg text-red-700">delete</span>
								</span>
								</div>

								<!-- <p class="text-red-500 text-xs italic">Please type a category name</p> -->
							</div>
							<div>
								<div class="flex">
									<div class="w-6/12 mx-2">
										<vue-the-mask mask="##:##:##"
															:class="`shadow appearance-none border rounded w-full py-2 px-3 ${$color('input')} ${$color('borderMain')}  leading-tight focus:outline-none focus:shadow-outline text-center`"
															type="text"
															placeholder="00:00"
															:value="secondsToStringTime(checkpoint.start)"
										/>
									</div>
									<div class="w-6/12 mx-2">
										<vue-the-mask mask="##:##:##"
															:class="`shadow appearance-none border rounded w-full py-2 px-3 ${$color('input')} ${$color('borderMain')}  leading-tight focus:outline-none focus:shadow-outline text-center`"
															type="text"
															placeholder="00:00"
															:value="secondsToStringTime(checkpoint.end)"
										/>
									</div>
								</div>
							</div>
						</div>
					</div>
					<div class="tc-block text-center">
						<span class="relative inline-flex rounded-md shadow-sm">
							<button
								v-if="!isCreatingTask"
								@click="save"
								class="bg-blue-500 mr-5 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline sm:mb-0 mb-5"
								type="button">
								<svg v-if="isSaving" class="animate-spin -ml-1 mr-3 h-5 w-5 text-white inline" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
									<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
									<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
								</svg>
								Save
							</button>
							<span v-if="isDataEdited" class="flex absolute h-5 w-5 top-0 right-0 -mt-1 mr-4">
								<span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"></span>
								<span class="relative inline-flex rounded-full h-5 w-5 bg-yellow-500"></span>
							</span>
						</span>

						<button v-if="isCreatingTask" @click="create"
										class="bg-orange-500 mr-5 hover:bg-orange-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline sm:mb-0 mb-5"
										type="button">
							Create
						</button>
						<button v-if="isCreatingTask" @click="cancel"
										class="bg-gray-500 mr-5 hover:bg-gray-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline sm:mb-0 mb-5"
										type="button">
							Cancel
						</button>
						<button
							v-if="!isCreatingTask"
							@click="addCheckpoint"
							class="bg-green-500 mr-5 hover:bg-green-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline"
							type="button">Add checkpoint</button>
					</div>
				</div>
				<transition name="fade">
					<Alert v-if="showSaveAlert" header="Saved" description="You saved your task"></Alert>
				</transition>
				<p v-if="form.approximately_time" class="text-gray-500 pl-4 pb-2">
					Estimated time to complete the task: {{ toHHMM(form.approximately_time) }}
				</p>
				<transition name="fade">
					<p v-if="showStatus" class="text-gray-500 float-right absolute right-0 -mt-8 mr-3">
						{{ statusText }}
					</p>
				</transition>
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import moment from 'moment'
	import InputField from "../UIElements/InputField"

	export default {
		name: 'TasksForm',
		components: {
			InputField
		},
		data() {
			return {
				savedData: {},
				availableSettings: [],
				settings: [],
				isSaving: false,
				autosaveTimeout: null,
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
				showSaveAlert: false,
				statusText: '',
				showStatus: false,
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
				form: this.getDefaultForm(),
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
							this.dispatchAutosave()
							return true
						}
					} else if (!this.equals(this.savedData[field], this.form[field])) {
						this.dispatchAutosave()
						return true
					}
				}
				return false
			}
		},
		async created () {
			if (this.taskId) {
				await this.loadModel()
				window.onkeydown = this.getListener()
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
						this.showStatusUI('Countdown stopped')
					})
				})
				.on('task-countdown-started', ({task}) => {
					const isCountdownStarted = !!this.form.start_time
					if (isCountdownStarted) {
						return
					}
					this.setFormDataWithDelay(task).then(() => {
						if (isCountdownStarted) {
							return
						}
						this.showStatusUI('Countdown started')
					})
				})
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
			setFormDataWithDelay(data, delay = 500) {
				return new Promise((resolve, reject) => {
					if (this.form.id !== data.id) {
						return
					}
					this.form.id = null
					setTimeout(() => {
						this.form = data
						resolve()
					}, delay)
				})
			},
			dispatchAutosave() {
				this.removeDispatchedAutosave()
				this.autosaveTimeout = setTimeout(() => this.save(true), 5000)
			},
			removeDispatchedAutosave() {
				if (!this.autosaveTimeout) {
					return
				}
				clearTimeout(this.autosaveTimeout)
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
				await this.save()
				await this.loadCategory()
			},
			getListener() {
				return (e) => {
					if (e.ctrlKey && (e.key.toLowerCase() === 's' || e.key.toLowerCase() === 'ы')) {
						e.preventDefault();
						this.save()
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
				const {data: {data}} = await this.$axios[this.form.start_time ? 'delete' : 'post'](`tasks/${this.taskId}/countdown`)
				this.form = {...data}
			},
			prepareForm() {
				if (this.form.project_category_id === '') {
					delete this.form.project_category_id
				}
			},
			async create() {
				try {
					this.prepareForm()
					const {data: {data}} = await this.$axios.post('tasks', this.form)
					if (!this.isCreatingTask) {
						this.showSavedAlert()
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
			showSavedAlert() {
				this.showSaveAlert = true
				setTimeout(() => this.showSaveAlert = false, 3000)
			},
			async save(autosave = false) {
				this.isSaving = true
				try {
					this.prepareForm()
					const {data: {data}} = await this.$axios.put(`tasks/${this.taskId}`, this.form)
					if (data.approximately_time) {
						this.approximatelyTime = this.toHHMM(data.approximately_time)
					}
					this.form = data
					if (autosave !== true) {
						this.showSavedAlert()
					}

					await this.saveSettings(this.settings)

					const id = this.form.id
					this.form.id = null
					setTimeout(() => this.form.id = id, 1)
				} catch (e) {
					if (e.response && e.response && e.response.data.errors) {
						this.errors = e.response.data.errors
					}
				}
				this.removeDispatchedAutosave()
				this.isSaving = false

				this.showStatusUI('Saved')
			},
			showStatusUI (text) {
				this.showStatus = true
				this.statusText = text
				setTimeout(() => this.showStatus = false, 5000)
			},
			cancel() {
				window.history.back();
			},
			goToCurrentTasks() {
				this.$router.push('/')
			},
			getDefaultForm() {
				return {
					title: '',
					status: 'created',
					project_category_id: this.projectCategoryId || '',
					description: '',
					common_time: 0,
					checkpoints: []
				};
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
				if (!this.form.checkpoints) {
					this.form.checkpoints = []
				}
				const currentTime = this.form.common_time
				if (this.form.checkpoints.length !== 0 && this.form.checkpoints.length !== 1) {
					this.form.checkpoints[this.form.checkpoints.length - 1].end = currentTime
				}
				this.form.checkpoints.push({
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
		beforeUnmount () {
			this.form = this.getDefaultForm()
			clearInterval(this.countdownInterval)
			clearInterval(this.reminderInterval)
		}
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
