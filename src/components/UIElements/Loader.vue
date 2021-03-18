<template>
	<span v-if="isMini" class="loader">
		<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="animate-spin" :class="miniLoaderClasses">
			<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
			<path fill="#fff" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
		</svg>
	</span>

	<span v-else>
		<svg
			viewBox="0 0 38 38"
			xmlns="http://www.w3.org/2000/svg"
			width="45"
			height="45"
			stroke="#3A4564">
			<g fill="none" fill-rule="evenodd">
				<g transform="translate(1 1)" stroke-width="2">
					<circle stroke-opacity=".25" cx="18" cy="18" r="18" />
					<path d="M36 18c0-9.94-8.06-18-18-18" transform="rotate(218.021 18 18)">
						<animateTransform attributeName="transform" type="rotate" from="0 18 18" to="360 18 18" dur="0.8s" repeatCount="indefinite" />
					</path>
				</g>
			</g>
		</svg>
	</span>
</template>

<script>
export default {
	name: "Loader",
	props: {
		isMini: {
			type: Boolean,
			required: false,
			default: false
		},
		isStatic: {
			type: Boolean,
			required: false,
			default: false
		},
		miniLoaderClasses: {
			type: String,
			required: false,
			default: ''
		}
	},
};
</script>

<style lang="scss" scoped>
	.loader {
		width: 18px;
		height: 18px;
		display: block;
	}
</style>
