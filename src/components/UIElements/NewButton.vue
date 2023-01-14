<template>
	<div class="inline-block button-custom-component">
		<router-link
			v-if="type === 'router-link'"
			:class="`bg-${color}-700 hover:bg-${color}-600`"
			:to="to"
			class="mt-3 sm:mt-0 py-2 px-5 md:py-3 md:px-6 font-bold text-white rounded-lg shadow-md"
		>
			<slot />
		</router-link>
		<button
			v-else
			:class="`bg-${color}-700 hover:bg-${color}-600 ${$color('buttonActionOpacity')}`"
			:type="type"
			class="mt-3 sm:mt-0 py-2 px-5 md:py-3 md:px-6 font-bold text-white rounded-lg shadow-md focus:outline-none"
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
			default: 'button'
		},
		to: {
			required: false,
			type: String,
			default: ''
		},
		color: {
			required: false,
			type: String,
			default: 'green'
		}
	}
};
</script>

<style lang="scss" scoped>
button {
	&:hover, &:focus, &:active {
		outline: none;
	}

	min-height: 45px;
	line-height: 1;
}
</style>
