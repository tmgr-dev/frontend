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
				class="dark:bg-gray-900 bg-white transition-colors duration-300 max-w-xl mx-auto mt-5 shadow-md rounded px-8 py-6"
			>
				<form class="w-full">
					<div class="mb-4">
						<label
							class="block text-gray-700 text-sm font-bold mb-2"
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

					<label class="block text-gray-700 text-sm font-bold mb-2">
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
								class="block text-gray-700 text-sm font-bold mb-2"
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
												class="b-switch-list__title text-gray-800 dark:text-gray-400"
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
							class="bg-blue-500 mr-5 hover:bg-blue-600 transition text-white font-bold py-2 px-4 rounded focus:outline-none"
							type="button"
							@click.prevent="create"
						>
							{{ isCreate ? 'Create' : 'Save' }}
						</button>

						<button
							v-if="isCreate"
							class="bg-orange-500 hover:bg-orange-600 transition text-white font-bold py-2 px-4 rounded focus:outline-none"
							type="button"
							@click.prevent="create(false)"
						>
							Add & Continue
						</button>

						<router-link
							v-if="!isCreate"
							:to="`/projects-categories/${form.id}/children`"
							class="bg-gray-500 hover:bg-gray-700 text-white font-bold py-2 px-4 rounded focus:outline-none"
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
				form: {
					title: '',
					project_category_id: this.$route.params.project_category_id || null,
					slug: '',
				},
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
				this.form = await getCategory(this.categoryId);
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
			async loadParentCategories() {
				const data = await getCategories();
				this.parentCategories = data.filter((category) => {
					return category.id !== this.form.id;
				});
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
				return this.parentCategories.find((category) => {
					return category.id === id;
				});
			},
			async create(withRoutePush = true) {
				if (!this.form.project_category_id) {
					delete this.form.project_category_id;
				}

				if (!this.isCreate) {
					this.form = await updateCategory(this.form.id, this.form);
					const settings = await updateCategorySettings(
						this.form.id,
						this.settings,
					);

					this.initSettings(this.availableSettings, settings);
					this.showAlert('Saved', 'The category was saved');

					return;
				}

				this.form.slug = this.generateSlugFromRu(this.form.title);
				this.form = await createCategory(this.form);

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
