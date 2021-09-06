<template>
	<div class="relative">
		<svg
			@click="opened = true"
			class="mr-3 md:mr-1 w-6 fill-current text-grey-dark"
			xmlns="http://www.w3.org/2000/svg"
			viewBox="0 0 20 20">
			<path d="M4 12a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4z" />
		</svg>

		<div v-if="opened" @click="opened = false" class="fixed inset-0 h-full w-full z-10"></div>

		<div v-if="opened" class="absolute right-0 mt-2 bg-white rounded-md shadow-lg overflow-hidden z-20" style="width:15rem;">
			<div class="py-2">
				<a
					v-for="action in actions"
					:href="action.link || '#'"
					@click.prevent="() => {action.click(); opened = false}"
					class="flex items-center px-4 py-3 border-b hover:bg-gray-100 -mx-2"
				>
					<p class="text-gray-600 text-sm mx-2">{{ action.label }}</p>
				</a>
			</div>
			<!--												<a href="#" class="block bg-gray-800 text-white text-center font-bold py-2">See all notifications</a>-->
		</div>
	</div>
</template>

<script>
export default {
	name: "DashboardDropdownMenu",
	props: {
		actions: {
			required: true,
			type: Array
		}
	},
	data () {
		return {
			opened: false
		}
	}
}
</script>

<style scoped>

</style>
