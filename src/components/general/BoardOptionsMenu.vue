<template>
	<!-- @todo close options by clicking outside	-->
	<div class="relative">
		<svg
			class="text-grey-dark mr-3 w-6 fill-current md:mr-1"
			viewBox="0 0 20 20"
			xmlns="http://www.w3.org/2000/svg"
			@click="opened = !opened"
		>
			<path
				d="M4 12a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4z"
			/>
		</svg>

		<div
			v-if="opened"
			class="absolute right-0 z-20 mt-2 w-40 overflow-hidden rounded-md bg-white shadow-lg"
		>
			<a
				v-for="action in actions"
				:href="action.link || '#'"
				class="flex items-center border-b px-3 py-3 last:border-b-0 hover:bg-gray-100"
				@click.prevent="
					() => {
						action.click();
						opened = false;
					}
				"
			>
				<span class="text-sm text-gray-600">{{ action.label }}</span>
			</a>
		</div>
	</div>
</template>

<script>
	export default {
		name: 'BoardOptionsMenu',
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
