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
			<rect x="0" y="0" rx="0" ry="0" width="400" height="32" />
			<rect x="0" y="35" rx="0" ry="0" width="400" height="32" />
			<rect x="0" y="70" rx="0" ry="0" width="400" height="32" />
			<rect x="0" y="105" rx="0" ry="0" width="400" height="32" />
			<rect x="0" y="140" rx="0" ry="0" width="400" height="32" />
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
		@media (max-width: 1279px) {
			height: 560px;
		}
	}
</style>
