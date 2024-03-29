<template>
	<BaseLayout>
		<template #header>{{ h1 }}</template>

		<template v-if="categories && categories.length > 0" #action>
			<breadcrumbs
				v-if="form.project_category_id"
				:current="'Edit category'"
				:items="getBreadcrumbs(getParents())"
			/>
		</template>

		<template #body>
			<div
				class="mx-auto mt-5 max-w-xl rounded bg-white px-8 py-6 shadow-md transition-colors duration-300 dark:bg-gray-900"
			>
				<form class="w-full">
					<div class="mb-4">
						<label
							class="mb-2 block text-sm font-bold text-gray-700"
							for="categoryName"
						>
							Project category name
						</label>

						<TextField
							v-model="form.title"
							:errors="errors.title"
							placeholder="Name"
						/>
					</div>

					<label class="mb-2 block text-sm font-bold text-gray-700">
						Parent category
					</label>

					<div class="relative mb-4">
						<Select
							v-model="form.project_category_id"
							:options="categories"
							label-key="title"
							value-key="id"
						/>
					</div>

					<div v-if="!isCreate">
						<SettingsLoader v-if="isLoading" class="mt-5" />

						<div v-for="(setting, index) in availableSettings">
							<label
								:for="`setting-${setting.id}`"
								class="mb-2 block text-sm font-bold text-gray-700"
							>
								{{ setting.name }}
							</label>

							<div class="relative mb-4">
								<Select
									v-if="!setting.show_custom_value_input"
									:options="setting.default_values"
									v-model="settings[index].value"
									:placeholder="setting.description"
									label-key="value"
									value-key="value"
								/>

								<div v-else-if="setting.custom_value_available">
									<TimeField
										v-if="setting.component_type === 'time_in_seconds'"
										v-model="settings[index].value"
										:placeholder="setting.description"
									/>

									<TextField
										v-else
										v-model="settings[index].value"
										:placeholder="setting.description"
									/>
								</div>

								<small v-if="!setting.show_custom_value_input">
									{{ setting.description }}
								</small>

								<Switcher
									v-if="
										setting.custom_value_available &&
										setting.default_values &&
										setting.default_values.length > 0
									"
									name="set_custom_value"
									v-model="setting.show_custom_value_input"
									placeholder="Set custom value"
								/>
							</div>
						</div>
					</div>

					<div class="mt-8 flex-row justify-center">
						<button
							class="mr-5 rounded bg-blue-500 py-2 px-4 font-bold text-white transition hover:bg-blue-600 focus:outline-none"
							type="button"
							@click.prevent="create"
						>
							{{ isCreate ? 'Create' : 'Save' }}
						</button>

						<button
							v-if="isCreate"
							class="rounded bg-orange-500 py-2 px-4 font-bold text-white transition hover:bg-orange-600 focus:outline-none"
							type="button"
							@click.prevent="create(false)"
						>
							Add & Continue
						</button>

						<router-link
							v-if="!isCreate"
							:to="`/projects-categories/${form.id}/children`"
							class="rounded bg-gray-500 py-2 px-4 font-bold text-white hover:bg-gray-700 focus:outline-none"
							type="button"
						>
							Cancel
						</router-link>
					</div>
				</form>
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import extractParents from 'src/utils/extractParents';
	import Breadcrumbs from 'src/components/general/Breadcrumbs.vue';
	import getBreadcrumbs from 'src/utils/getBreadcrumbs';
	import SettingsLoader from 'src/components/loaders/SettingsLoader.vue';
	import {
		createCategory,
		getCategories,
		getCategory,
		updateCategory,
	} from 'src/actions/tmgr/categories';
	import {
		getCategorySettings,
		updateCategorySettings,
	} from 'src/actions/tmgr/settings';
	import generateSlugFromRu from 'src/utils/generateSlugFromRu';
	import Select from 'src/components/general/Select.vue';
	import Switcher from 'src/components/general/Switcher.vue';
	import TextField from 'src/components/general/TextField.vue';
	import TimeField from 'src/components/general/TimeField.vue';

	export default {
		name: 'ProjectCategoryForm',
		components: {
			TimeField,
			TextField,
			Switcher,
			Select,
			SettingsLoader,
			Breadcrumbs,
		},
		data() {
			return {
				h1: null,
				form: {
					title: '',
					project_category_id: this.$route.params.project_category_id || null,
					slug: '',
				},
				errors: {},
				categories: [],
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
				this.form = await getCategory(this.categoryId);
			}
			await this.loadCategories();
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
			async loadCategories() {
				const data = await getCategories();
				this.categories = data.filter(
					(category) => category.id !== this.categoryId,
				);
			},
			async loadProjectCategorySettings() {
				try {
					const data = await getCategorySettings();

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
				return this.categories.find((category) => {
					return category.id === id;
				});
			},
			async create(withRoutePush = true) {
				if (!this.form.project_category_id) {
					delete this.form.project_category_id;
				}

				this.errors = {};

				if (!this.isCreate) {
					this.form = await updateCategory(this.form.id, this.form);

					try {
						const settings = await updateCategorySettings(
							this.form.id,
							this.settings,
						);

						this.initSettings(this.availableSettings, settings);
						this.showAlert('Saved', 'The category was saved');
					} catch (e) {
						this.errors = e.response?.data?.errors;
						throw e;
					}

					return;
				}

				this.form.slug = this.generateSlugFromRu(this.form.title);
				try {
					this.form = await createCategory(this.form);
				} catch (e) {
					this.errors = e.response?.data?.errors;
					throw e;
				}

				if (withRoutePush) {
					await this.$router.push(
						`/projects-categories/${this.form.id}/children`,
					);
				}
			},
			setFormTexts() {
				this.h1 = `${this.getFormTitlePrefix()} category`;
			},
			getFormTitlePrefix() {
				return this.isCreate ? 'Add' : 'Edit';
			},
			generateSlugFromRu,
		},
	};
</script>
