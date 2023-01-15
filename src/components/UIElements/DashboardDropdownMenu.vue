<template>
	<div class="relative">
		<svg
			class="mr-3 md:mr-1 w-6 fill-current text-grey-dark"
			viewBox="0 0 20 20"
			xmlns="http://www.w3.org/2000/svg"
			@click="opened = true"
		>
			<path
				d="M4 12a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4z"
			/>
		</svg>

		<div
			v-if="opened"
			class="fixed inset-0 h-full w-full z-10"
			@click="opened = false"
		></div>

		<div
			v-if="opened"
			class="absolute right-0 mt-2 bg-white rounded-md shadow-lg overflow-hidden z-20"
			style="width: 15rem"
		>
			<div class="py-2">
				<a
					v-for="action in actions"
					:href="action.link || '#'"
					class="flex items-center px-4 py-3 border-b hover:bg-gray-100 -mx-2"
					@click.prevent="
						() => {
							action.click();
							opened = false;
						}
					"
				>
					<p class="text-gray-600 text-sm mx-2">{{ action.label }}</p>
				</a>
			</div>
		</div>
	</div>
</template>

<script>
	export default {
		name: 'DashboardDropdownMenu',
		props: {
			actions: {
				required: true,
				type: Array,
			},
		},
		data() {
			return {
				opened: false,
			};
		},
	};
</script>

<style scoped></style>
