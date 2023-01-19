<template>
	<BaseLayout>
		<template #header>{{ h1 }}</template>

		<template v-if="parentCategories && parentCategories.length > 0" #action>
			<breadcrumbs
				v-if="form.project_category_id"
				:current="'Edit category'"
				:drop="drop"
				:items="getBreadcrumbs(getParents())"
			/>
		</template>

		<template #body>
			<div
				:class="`${$color(
					'blocks',
				)} max-w-xl mx-auto mt-5 shadow-md rounded px-8 py-6`"
			>
				<form class="w-full">
					<div class="mb-4">
						<label
							class="tc-block text-gray-700 text-sm font-bold mb-2"
							for="categoryName"
						>
							Project category name
						</label>

						<input-field
							id="categoryName"
							v-model="form.title"
							placeholder="Name"
							type="text"
						/>
					</div>

					<label class="tc-block text-gray-700 text-sm font-bold mb-2">
						Parent category
					</label>

					<div class="relative mb-4">
						<input-field
							v-model="form.project_category_id"
							:options="parentCategories"
							option-name-key="title"
							option-value-key="id"
							placeholder="test"
							type="select"
						/>
					</div>

					<div v-if="!isCreate">
						<SettingsLoader v-if="isLoading" class="mt-5" />

						<div v-for="(setting, index) in availableSettings">
							<label
								:for="`setting-${setting.id}`"
								class="tc-block text-gray-700 text-sm font-bold mb-2"
							>
								{{ setting.name }}
							</label>

							<div class="relative mb-4">
								<input-field
									v-if="!setting.show_custom_value_input"
									:id="`setting-${setting.id}`"
									v-model="settings[index].value"
									:options="setting.default_values"
									:placeholder="setting.description"
									:tag="(settings[index].id = setting.id)"
									option-name-key="value"
									option-value-key="value"
									type="select"
								/>

								<div v-else-if="setting.custom_value_available">
									<input-field
										:id="`setting-${setting.id}`"
										v-model="settings[index].value"
										:placeholder="setting.description"
										:tag="(settings[index].id = setting.id)"
										:type="setting.component_type"
									/>
								</div>

								<small v-if="!setting.show_custom_value_input">
									{{ setting.description }}
								</small>

								<div
									v-if="setting.custom_value_available"
									class="b-switch-list mt-3"
								>
									<div
										v-if="
											setting.default_values &&
											setting.default_values.length > 0
										"
										class="b-switch-list__item"
									>
										<label class="b-switch">
											<input
												v-model="setting.show_custom_value_input"
												name="show_tooltips"
												type="checkbox"
												@change="settings[index].value = ''"
											/>
											<span></span>
										</label>

										<div class="b-switch-list__text">
											<div
												:class="$color('settingsTextColor')"
												class="b-switch-list__title"
											>
												Set custom value
											</div>
										</div>
									</div>
								</div>
							</div>
						</div>
					</div>

					<div class="flex-row justify-center mt-8">
						<button
							class="bg-blue-500 mr-5 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline"
							type="button"
							@click.prevent="create"
						>
							{{ isCreate ? 'Create' : 'Save' }}
						</button>

						<button
							v-if="isCreate"
							class="bg-orange-500 hover:bg-orange-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline"
							type="button"
							@click.prevent="createAndContinue"
						>
							Add & Continue
						</button>

						<router-link
							v-if="!isCreate"
							:to="`/projects-categories/${form.id}/children`"
							class="bg-gray-500 hover:bg-gray-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline"
							type="button"
						>
							Cancel
						</router-link>
					</div>
				</form>

				<alert ref="alert" />
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import extractParents from 'src/utils/extractParents';
	import InputField from 'src/components/UIElements/InputField';
	import Breadcrumbs from 'src/components/UIElements/Breadcrumbs';
	import getBreadcrumbs from 'src/utils/breadcrumbs/getBreadcrumbs';
	import SettingsLoader from 'src/components/Loaders/SettingsLoader.vue';

	export default {
		name: 'ProjectCategoryForm',
		components: {
			SettingsLoader,
			Breadcrumbs,
			InputField,
		},
		data() {
			return {
				h1: null,
				form: this.getDefaultForm(),
				parentCategories: [],
				availableSettings: [],
				settings: [],
				isLoading: true,
			};
		},
		watch: {
			settings: {
				handler(newVal, oldVal) {},
				deep: true,
			},
		},
		computed: {
			isCreate() {
				return !this.$route.params.id && !this.form.id;
			},
			categoryId() {
				return this.$route.params.id;
			},
		},
		async mounted() {
			this.setFormTexts();

			if (!this.isCreate) {
				await this.loadModel();
			}
			await this.loadParentCategories();
			await this.loadProjectCategorySettings();
		},
		methods: {
			getBreadcrumbs,
			extractParents,
			getParents() {
				const parents = this.extractParents({
					...this.form,
					...{
						parent_category: this.findProjectCategoryById(
							this.form.project_category_id,
						),
					},
				});
				parents.push(this.form);
				return parents;
			},
			async loadModel() {
				const {
					data: { data },
				} = await this.$axios.get(`project_categories/${this.categoryId}`);
				this.form = data;
			},
			async loadParentCategories() {
				const {
					data: { data },
				} = await this.$axios.get('project_categories?all');
				this.parentCategories = data.filter((category) => {
					return category.id !== this.form.id;
				});
			},
			async loadProjectCategorySettings() {
				try {
					const {
						data: { data },
					} = await this.$axios.get('project_categories/settings');

					this.initSettings(data, this.form.settings);
					this.availableSettings = data;
				} catch (e) {
					console.error(e);
				} finally {
					this.isLoading = false;
				}
			},
			initSettings(availableSettings, settings = []) {
				return availableSettings.map((item, index) => {
					const setting = this.getSettingById(settings, item.id, {
						id: item.id,
						value: '',
					});
					this.settings[index] = setting;
					item.show_custom_value_input =
						item.default_values &&
						item.default_values.findIndex(
							(val) => val.value === setting.value,
						) === -1;
				});
			},
			getSettingById(settings, id, defaultResult = null) {
				return settings.find((setting) => setting.id === id) || defaultResult;
			},
			findProjectCategoryById(id) {
				return this.parentCategories.find((category) => {
					return category.id === id;
				});
			},
			async create(withRoutePush = true) {
				if (!this.form.project_category_id) {
					delete this.form.project_category_id;
				}

				if (!this.isCreate) {
					const {
						data: { data },
					} = await this.$axios.put(
						`project_categories/${this.form.id}`,
						this.form,
					);
					this.form = data;
					await this.saveSettings(this.settings);
					this.showAlert('Saved', 'The category was saved');
					return;
				}
				this.form.slug = this.generateSlug(this.form.title);

				const {
					data: { data },
				} = await this.$axios.post('project_categories', this.form);
				this.form = data;
				if (!withRoutePush) {
					return;
				}
				await this.$router.push(`/projects-categories/${data.id}/children`);
			},
			async saveSettings(settings) {
				const {
					data: { data },
				} = await this.$axios.put(
					`/project_categories/${this.form.id}/settings`,
					settings,
				);
				this.initSettings(this.availableSettings, data.settings);
			},
			async createAndContinue() {
				await this.create(false);
				this.form = this.getDefaultForm();
			},
			generateSlug(text) {
				const ru = {
						а: 'a',
						б: 'b',
						в: 'v',
						г: 'g',
						д: 'd',
						е: 'e',
						ё: 'e',
						ж: 'j',
						з: 'z',
						и: 'i',
						к: 'k',
						л: 'l',
						м: 'm',
						н: 'n',
						о: 'o',
						п: 'p',
						р: 'r',
						с: 's',
						т: 't',
						у: 'u',
						ф: 'f',
						х: 'h',
						ц: 'c',
						ч: 'ch',
						ш: 'sh',
						щ: 'shch',
						ы: 'y',
						э: 'e',
						ю: 'u',
						я: 'ya',
						' ': '-',
					},
					n_str = [];

				text = text.replace(/[ъь]+/g, '').replace(/й/g, 'i');

				for (let i = 0; i < text.length; ++i) {
					n_str.push(
						ru[text[i]] ||
							(ru[text[i].toLowerCase()] === undefined && text[i]) ||
							ru[text[i].toLowerCase()].replace(/^(.)/, function (match) {
								return match.toUpperCase();
							}),
					);
				}
				return n_str.join('').toLowerCase();
			},
			setFormTexts() {
				this.h1 = `${this.getFormTitlePrefix()} category`;
			},
			getFormTitlePrefix() {
				return this.isCreate ? 'Add' : 'Edit';
			},
			getDefaultForm() {
				return {
					title: '',
					project_category_id: this.$route.params.project_category_id || null,
					slug: '',
				};
			},
		},
	};
</script>
