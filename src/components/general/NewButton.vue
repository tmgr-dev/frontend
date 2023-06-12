<template>
	<div class="button-custom-component inline-block">
		<router-link
			v-if="type === 'router-link'"
			:to="to"
			class="mt-3 rounded-lg py-2 px-5 font-bold text-white shadow-md sm:mt-0 md:py-3 md:px-6"
			:class="buttonClass"
		>
			<slot />
		</router-link>

		<button
			v-else
			:type="type"
			class="mt-3 rounded-lg py-2 px-5 font-bold text-white shadow-md focus:outline-none dark:opacity-50 hover:dark:opacity-75 sm:mt-0 md:py-3 md:px-6"
			:class="buttonClass"
		>
			<slot />
		</button>
	</div>
</template>

<script>
	export default {
		name: 'NewButton',
		props: {
			type: {
				required: false,
				type: String,
				default: 'button',
			},
			to: {
				required: false,
				type: String,
				default: '',
			},
			buttonClass: {
				required: false,
				type: String,
				default: 'bg-green-700 hover:bg-green-600',
			},
		},
	};
</script>

<style lang="scss" scoped>
	button {
		&:hover,
		&:focus,
		&:active {
			outline: none;
		}

		min-height: 45px;
		line-height: 1;
	}
</style>
