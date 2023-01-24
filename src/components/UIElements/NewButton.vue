<template>
	<div class="inline-block button-custom-component">
		<router-link
			v-if="type === 'router-link'"
			:to="to"
			class="mt-3 sm:mt-0 py-2 px-5 md:py-3 md:px-6 font-bold text-white rounded-lg shadow-md"
			:class="buttonClass"
		>
			<slot />
		</router-link>

		<button
			v-else
			:type="type"
			class="mt-3 sm:mt-0 py-2 px-5 md:py-3 md:px-6 dark:opacity-50 hover:dark:opacity-75 font-bold text-white rounded-lg shadow-md focus:outline-none"
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
