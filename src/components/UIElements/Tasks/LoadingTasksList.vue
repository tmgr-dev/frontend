<template>
	<div class="overflow-hidden">
		<ContentLoader
			class="task-list-loading"
			:speed="0.875"
			viewBox="0 0 400 180"
			primaryColor="#1a202c"
			secondaryColor="#2b3446"
			preserveAspectRatio="none"
		>
			<rect x="0" y="0" rx="0" ry="0" width="400" height="28" />
			<rect x="0" y="31" rx="0" ry="0" width="400" height="28" />
			<rect x="0" y="62" rx="0" ry="0" width="400" height="28" />
			<rect x="0" y="93" rx="0" ry="0" width="400" height="28" />
			<rect x="0" y="124" rx="0" ry="0" width="400" height="28" />
		</ContentLoader>
	</div>
</template>

<script>
	import { ContentLoader } from 'vue-content-loader'

	export default {
		name: 'LoadingTasksList',
		props: {},
		components: {
			ContentLoader
		}

	}
</script>

<style lang="scss" scoped>
	.task-list-loading {
		height: 593px;

		@media (max-width: 1279px) {
			height: 560px;
		}
	}
</style>
