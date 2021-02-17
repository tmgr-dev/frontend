<template>
	<BaseLayout>
		<template #header>{{ h1 }}</template>

		<template #body>
			<div :class="`${$color('blocks')} max-w-xl mx-auto mt-5 shadow-md rounded px-8 py-6`">
				<form class="w-full">
					<div class="mb-4">
						<label class="block text-gray-700 text-sm font-bold mb-2" for="categoryName">
							Project category name
						</label>
						<input
							class="shadow appearance-none border rounded w-full py-2 px-3 border-gray-300 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
							id="categoryName" type="text" placeholder="Name" v-model="form.title">
						<!-- <p class="text-red-500 text-xs italic">Please type a category name</p> -->
					</div>

					<label for="parent-category" class="block text-gray-700 text-sm font-bold mb-2">
						Parent category
					</label>
					<div class="relative mb-4">
						<select id="parent-category"
										class="block appearance-none w-full bg-white border border-gray-300 px-4 py-2 pr-8 rounded shadow leading-tight focus:outline-none focus:shadow-outline"
										v-model="form.project_category_id">
							<option value="" class="text-gray-500">Choose parent category</option>
							<option v-for="(c, i) in parentCategories" :key="i" :value="c.id">
								{{ c.title }}
							</option>
						</select>
						<div class="pointer-events-none absolute inset-y-0 right-0 flex items-center px-2 text-gray-700">
							<svg class="fill-current h-4 w-4" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20">
								<path d="M9.293 12.95l.707.707L15.657 8l-1.414-1.414L10 10.828 5.757 6.586 4.343 8z"/>
							</svg>
						</div>
					</div>
					<hr class="py-2">
					<label for="parent-category" class="block text-gray-700 text-sm font-bold mb-5">
						Settings
					</label>
					<div>
						<div v-for="(setting, index) in availableSettings">
							<label :for="`setting-${setting.id}`" class="block text-gray-700 text-sm font-bold mb-2">
								{{ setting.name }}
							</label>
							<div class="relative mb-4">
								<select :id="`setting-${setting.id}`"
												v-if="!setting.show_custom_value_input"
												class="block appearance-none w-full bg-white border border-gray-300 px-4 py-2 pr-8 rounded shadow leading-tight focus:outline-none focus:shadow-outline"
												v-model="settings[index].value">
									<option value="" class="text-gray-500">Choose default value</option>
									<option v-for="(c, i) in setting.default_values" :key="i" :value="c.value">
										{{ c.value }}
									</option>
								</select>
								<input
									v-else-if="setting.custom_value_available"
									class="shadow appearance-none border rounded w-full py-2 px-3 border-gray-300 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
									:id="`setting-${setting.id}`" type="text" :placeholder="setting.description" v-model="settings[index].value" :tag="settings[index].id = setting.id">
								<small v-if="!setting.show_custom_value_input">{{ setting.description }}</small>
								<div class="b-switch-list" v-if="setting.custom_value_available">
									<div class="b-switch-list__item">
										<label class="b-switch">
											<input type="checkbox" name="show_tooltips" v-model="setting.show_custom_value_input" @change="settings[index].value = ''">
											<span></span>
										</label>
										<div class="b-switch-list__text">
											<div class="b-switch-list__title" :class="$color('settingsTextColor')">Set custom value</div>
										</div>
									</div>
								</div>
							</div>
						</div>
					</div>

					<div class="flex-row justify-center mt-8">
						<button @click.prevent="create"
										class="bg-blue-500 mr-5 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline"
										type="button">
							{{ h1 }}
						</button>
						<button v-if="isCreate" @click.prevent="createAndContinue"
										class="bg-orange-500 hover:bg-orange-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline"
										type="button">
							Add & Continue
						</button>
						<router-link
							v-if="!isCreate" :to="`/projects-categories/${form.id}/children`"
							class="bg-orange-500 hover:bg-orange-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline"
							type="button">
							View
						</router-link>
					</div>
				</form>
				<transition name="fade">
					<Alert v-if="showSaveAlert" header="Saved" description="You saved your project category"/>
				</transition>
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	export default {
		name: 'ProjectCategoryForm',
		components: {},
		props: [],
		data() {
			return {
				h1: null,
				showSaveAlert: false,
				form: this.getDefaultForm(),
				parentCategories: null,
				availableSettings: [],
				settings: [],
			}
		},
		watch: {
			settings: {
				handler (newVal, oldVal) {
					console.log(newVal[0])
				},
				deep: true
			}
		},
		computed: {
			isCreate() {
				return !this.$route.params.id && !this.form.id
			},
			categoryId () {
				return this.$route.params.id
			}
		},
		async mounted() {
			this.setFormTexts()
			await this.loadParentCategories()

			if (!this.isCreate) {
				await this.loadModel()
			}
			await this.loadProjectCategorySettings()
		},
		methods: {
			showSavedAlert() {
				this.showSaveAlert = true
				setTimeout(() => this.showSaveAlert = false, 3000)
			},
			async loadModel() {
				const {data: {data}} = await this.$axios.get(`project_categories/${this.categoryId}`)
				this.form = data
			},
			async loadParentCategories() {
				const {data: {data}} = await this.$axios.get('project_categories?all')
				this.parentCategories = data
			},
			async loadProjectCategorySettings() {
				const {data: {data}} = await this.$axios.get('project_categories/settings')
				this.initSettings(data, this.form.settings)
				this.availableSettings = data
			},
			initSettings(availableSettings, settings = []) {
				availableSettings.forEach((item, index) => {
					console.log(settings, this.getSettingById(settings, item.id), item.id)
					this.settings[index] = this.getSettingById(settings, item.id, {
						id: item.id,
						value: ''
					})
				})
			},
			getSettingById(settings, id, defaultResult = null) {
				return settings.find(setting => setting.id === id) || defaultResult
			},
			async create(withRoutePush = true) {
				if (!this.form.project_category_id) {
					delete this.form.project_category_id
				}

				if (!this.isCreate) {
					const {data: {data}} = await this.$axios.put(`project_categories/${this.form.id}`, this.form)
					this.form = data
					await this.saveSettings(this.settings)
					this.showSavedAlert()
					return
				}
				this.form.slug = this.generateSlug(this.form.title)

				await this.$axios.post('project_categories', this.form)
				if (!withRoutePush) {
					return
				}
				await this.$router.push('/projects-categories')
			},
			async saveSettings(settings) {
				const {data: {data}} = await this.$axios.put(`/project_categories/${this.form.id}/settings`, settings)
				this.initSettings(this.availableSettings, data.settings)
			},
			async createAndContinue() {
				await this.create(false)
				this.form = this.getDefaultForm()
			},
			generateSlug(text) {
				const ru = {
					'а': 'a', 'б': 'b', 'в': 'v', 'г': 'g', 'д': 'd',
					'е': 'e', 'ё': 'e', 'ж': 'j', 'з': 'z', 'и': 'i',
					'к': 'k', 'л': 'l', 'м': 'm', 'н': 'n', 'о': 'o',
					'п': 'p', 'р': 'r', 'с': 's', 'т': 't', 'у': 'u',
					'ф': 'f', 'х': 'h', 'ц': 'c', 'ч': 'ch', 'ш': 'sh',
					'щ': 'shch', 'ы': 'y', 'э': 'e', 'ю': 'u', 'я': 'ya', ' ': '-'
				}, n_str = [];

				text = text.replace(/[ъь]+/g, '').replace(/й/g, 'i');

				for (var i = 0; i < text.length; ++i) {
					n_str.push(
						ru[text[i]]
						|| ru[text[i].toLowerCase()] == undefined && text[i]
						|| ru[text[i].toLowerCase()].replace(/^(.)/, function (match) {
							return match.toUpperCase()
						})
					);
				}

				return n_str.join('').toLowerCase();
			},
			setFormTexts() {
				this.h1 = `${this.getFormTitlePrefix()} category`
			},
			getFormTitlePrefix() {
				return this.isCreate ? 'Add' : 'Edit'
			},
			getDefaultForm() {
				return {
					title: '',
					project_category_id: this.$route.params.project_category_id || null,
					slug: '',
				}
			}
		}
	}
</script>
