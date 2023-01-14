<template>
	<modal
		:close-on-bg-click="false"
		:is-center="true"
		:modal-width="500"
		@close="$emit('close')">
		<template #modal-body>
			<form @submit.prevent="$emit('submit')">
				<div>
					<label
						:class="`block text-sm text-left font-bold bg-gray-400 mb-2 mt-2 text-black ${$color('taskSettingTextColor')}`"
						for="">
						Category
						<vue-select
							v-model="selectedCategoryValue"
							:options="categories"
							label="title"
						/>
					</label>
				</div>
				<div>
					<label
						:class="`block text-sm text-left font-bold bg-gray-400 mb-2 mt-2 text-black ${$color('taskSettingTextColor')}`"
						for="">
						Estimated time
						<input-field
							v-model="form.approximately_time"
							:errors="errors.approximately_time"
							extra-class="bg-gray-400"
							placeholder="Enter approximately time"
							type="time_in_seconds"
						/>
					</label>
				</div>

				<b class="tc-block text-gray-900 text-base mt-5 font-bold mb-2">
					Settings
				</b>
				<div>
					<div v-for="(setting, index) in settings" id="settings">
						<label :for="`setting-${setting.id}`" class="tc-block text-gray-700 text-sm font-bold mb-2">
							{{ setting.name }}
						</label>
						<div class="relative mb-4">
							<select
								v-if="!setting.show_custom_value_input"
								:id="`setting-${setting.id}`"
								v-model="settings[index].value"
								class="tc-block appearance-none w-full bg-white border border-gray-300 px-4 py-2 pr-8 rounded shadow leading-tight focus:outline-none focus:shadow-outline">
								<option class="text-gray-500" value="">Choose default value</option>
								<option v-for="(c, i) in setting.default_values" :key="i" :value="c.value">
									{{ c.value }}
								</option>
							</select>

							<div v-else-if="setting.custom_value_available">
								<input-field
									:id="`setting-${setting.id}`"
									v-model="settings[index].value"
									:extra-class="`bg-gray-400  ${$color('taskSettingTextColor')}`"
									:placeholder="setting.description"
									:tag="settings[index].id = setting.id"
									:type="setting.component_type"
								/>
							</div>

							<small v-if="!setting.show_custom_value_input">{{ setting.description }}</small>

							<div v-if="setting.custom_value_available" class="b-switch-list">

								<div
									v-if="setting.default_values && setting.default_values.length > 0"
									class="b-switch-list__item">
									<label class="b-switch">
										<input v-model="setting.show_custom_value_input" name="show_tooltips" type="checkbox"
													 @change="settings[index].value = ''">
										<span></span>
									</label>
									<div class="b-switch-list__text">
										<div :class="$color('settingsTextColor')" class="b-switch-list__title">Set custom value</div>
									</div>
								</div>

							</div>

						</div>
					</div>
				</div>

				<div class="flex flex-nowrap items-center mt-5">
					<button
						class="tc-block w-2/4 mr-1 bg-gray-700 text-white p-2 rounded"
						type="button"
						@click="$emit('close')">
						Cancel
					</button>
					<button
						class="tc-block w-2/4 mr-1 bg-blue-700 text-white p-2 rounded"
						type="submit">
						Update
					</button>
				</div>
			</form>
		</template>
	</modal>
</template>

<script>
export default {
	name: 'TaskSettingsModal',
	emits: [
		'submit',
		'close',
		'update:selected-category'
	],
	props: {
		categories: {
			type: Array,
			required: true
		},
		selectedCategory: {
			required: true
		},
		settings: {
			type: Array,
			required: true
		},
		form: {
			type: Object,
			required: true
		},
		errors: {
			type: Object,
			required: true
		}
	},
	computed: {
		selectedCategoryValue: {
			get() {
				return this.selectedCategory;
			},
			set(v) {
				this.$emit('update:selected-category', v);
			}
		}
	}
};
</script>
