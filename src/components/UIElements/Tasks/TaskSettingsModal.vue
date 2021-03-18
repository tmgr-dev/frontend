<template>
	<modal
		:modal-width="500"
		:is-center="true"
		:close-on-bg-click="false"
		@close="$emit('close')">
		<template #modal-body>
			<form @submit.prevent="$emit('submit')">
				<div>
					<label :class="`block text-sm text-left font-bold bg-gray-400 mb-2 mt-2 text-black ${$color('taskSettingTextColor')}`" for="">
						Category
						<vue-select
							label="title"
							:options="categories"
							v-model="selectedCategoryValue"
						/>
					</label>
				</div>
				<div>
					<label :class="`block text-sm text-left font-bold bg-gray-400 mb-2 mt-2 text-black ${$color('taskSettingTextColor')}`" for="">
						Estimated time
						<input-field
							extra-class="bg-gray-400"
							v-model="form.approximately_time"
							:errors="errors.approximately_time"
							type="time_in_seconds"
							placeholder="Enter approximately time"
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
								:id="`setting-${setting.id}`"
								v-if="!setting.show_custom_value_input"
								class="tc-block appearance-none w-full bg-white border border-gray-300 px-4 py-2 pr-8 rounded shadow leading-tight focus:outline-none focus:shadow-outline"
								v-model="settings[index].value">
								<option value="" class="text-gray-500">Choose default value</option>
								<option v-for="(c, i) in setting.default_values" :key="i" :value="c.value">
									{{ c.value }}
								</option>
							</select>

							<div v-else-if="setting.custom_value_available">
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
									v-if="setting.default_values && setting.default_values.length > 0">
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

				<div class="flex flex-nowrap items-center mt-5">
					<button
						type="button"
						@click="$emit('close')"
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
				get () {
					return this.selectedCategory
				},
				set (v) {
					this.$emit('update:selected-category', v)
				}
			}
		}
	}
</script>
