<template>
	<BaseLayout>
		<template #header>
			Settings
		</template>
		<template #body>
				<div class="w-full p-5">
					<div :class="`${$color('blocks')} rounded-lg relative`">
						<div class="lg:flex">
							<div class="lg:w-full h-full mt-5 p-5">
								<label class="block text-sm text-left font-bold mb-2">
									Hourly Rate
								</label>
								<input-field
									:value.sync="form.settings.hourlyRate"
									:errors="errors.description"
									type="number"
									placeholder="Description"
								/>
							</div>
							<div class="lg:w-full h-full mt-5 p-5">
								<label class="block text-sm text-left font-bold mb-2">
									Night&Day mode
								</label>
								<day-night-switch :value.sync="switchOn"/>
							</div>
						</div>
						<div class="lg:flex">
							<div class="w-full h-full mt-5 p-5">
								<div class="block text-center">
									<button
										@click="save"
										class="bg-blue-500 mr-5 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline sm:mb-0 mb-5"
										type="button">
										Save
									</button>
								</div>
							</div>
						</div>
					</div>
				</div>
				<transition name="fade">
					<Alert v-if="showSaveAlert" header="Saved" description="You saved your task"></Alert>
				</transition>
		</template>
	</BaseLayout>
</template>

<script>
	import BaseLayout from "@/components/Layouts/BaseLayout";
	import DayNightSwitch from "./UIElements/DayNightSwitch";

	export default {
		name: 'Settings',
		components: {
			DayNightSwitch,
			BaseLayout
		},
		computed: {
			switchOn: {
				get () {
					return this.$store.getters.colorScheme === 'dark'
				},
				set (newValue) {
					this.$store.commit('colorScheme', newValue ? 'dark' : 'default')
				}
			}
		},
		data () {
			return {
				showSaveAlert: false,
				form: {
					settings: {
						hourlyRate: 450,
						colorScheme: 'default'
					}
				},
				errors: {}
			}
		},
		methods: {
			async save () {
				this.form.settings.colorScheme = this.$store.getters.colorScheme
				await this.$axios.put('/user/settings', this.form)
			}
		}
	}
</script>
