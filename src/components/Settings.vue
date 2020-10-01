<template>
	<BaseLayout>
		<template #header>
			Settings
		</template>
		<template #body>

			<div class="settings-container">
				<div class="b-switch-list">
					<div class="b-switch-list__item">
						<label class="b-switch">
							<input type="checkbox" name="show_tooltips" >
							<span></span>
						</label>
						<div class="b-switch-list__text">
							<div class="b-switch-list__title" :class="$color('settingsTextColor')">Show tooltips</div>
						</div>
					</div>
				</div>

				<div class="text-right">
					<button
						@click="updateSettings"
						class="bg-blue-500 mr-5 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded focus:outline-none sm:mb-0 mt-10"
						type="button">
						Save
					</button>
				</div>
			</div>

		</template>
	</BaseLayout>
</template>

<script>
	import BaseLayout from "@/components/Layouts/BaseLayout";

	export default {
		name: 'Settings',
		components: {
			BaseLayout
		},
		data: () => ({

		}),
		computed: {
			userSettings () {
				return this.$store.getters.getUserSettings
			}
		},
		methods: {
			async updateSettings () {
				try {
					await this.$axios.put('user/settings', {
						settings: this.userSettings
					})
				} catch (e) {
					console.error(e)
				}
			}
		}
	}
</script>

<style scoped>
	.settings-container {
		max-width: 700px;
		margin: 50px auto;
		padding: 20px;
		box-shadow: rgb(233 233 233) 1px 4px 20px;
	}
</style>
