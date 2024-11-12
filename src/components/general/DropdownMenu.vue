<template>
	<div
		v-if="isOpenDropdownMenu"
		class="fixed inset-0 z-20 h-full w-full cursor-default bg-black opacity-50"
		@click="isOpenDropdownMenu = false"
	/>

	<div class="z-10 lg:hidden">
		<button @click.prevent="isOpenDropdownMenu = !isOpenDropdownMenu">
			<svg
				class="text-grey-dark mr-3 h-12 w-6 fill-current md:mr-1"
				viewBox="0 0 20 20"
				xmlns="http://www.w3.org/2000/svg"
			>
				<path
					d="M4 12a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4z"
				/>
			</svg>
		</button>
	</div>

	<div
		v-if="isOpenDropdownMenu"
		class="absolute right-0 z-50 mt-2 w-48 rounded-lg bg-white py-2"
		style="top: 80px"
	>
		<a
			v-for="(action, i) in actions"
			:key="i"
			class="block px-4 py-2 text-gray-800 hover:bg-indigo-500 hover:text-white"
			href="#"
			@click.prevent="doAction(action)"
		>
			{{ action.label }}
		</a>
	</div>
</template>

<script>
	export default {
		name: 'DropdownMenu',
		props: {
			actions: {
				required: true,
				type: Array,
			},
		},
		data: () => ({
			isOpenDropdownMenu: false,
		}),
		methods: {
			doAction(action) {
				action.click();
				setTimeout(() => (this.isOpenDropdownMenu = false), 100);
			},
		},
	};
</script>
