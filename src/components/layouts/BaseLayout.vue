<template>
	<div class="flex min-h-screen w-screen flex-col">
		<header class="container relative mx-auto my-5 px-4">
			<h1
				class="relative mt-6 pt-1 text-center text-center text-2xl text-blue-800 dark:text-white md:mt-0 md:pt-0 md:text-left md:text-3xl"
			>
				<slot name="header" />
			</h1>

			<slot name="action" />
		</header>

		<div :class="bodyContainerClass">
			<slot name="body" />
		</div>

		<slot name="footer" />

		<div
			v-if="!noCopyright"
			class="container mx-auto mb-5 mt-auto px-5 pb-5 text-center text-gray-600"
		>
			<p>
				Task manager.
				<a class="text-gray-500" href="#" target="_blank">Tmgr.dev</a>
			</p>

			<p>{{ copyright }}</p>
		</div>
	</div>
</template>

<script>
	export default {
		name: 'BaseLayout',
		props: {
			noCopyright: {
				type: Boolean,
				required: false,
				default: false,
			},
			bodyContainerClass: {
				type: String,
				required: false,
				default: 'container mx-auto px-2',
			},
		},
		computed: {
			copyright() {
				const startYear = 2020;
				const currentYear = new Date().getFullYear();

				return startYear === currentYear
					? currentYear
					: `${startYear} - ${currentYear}`;
			},
		},
	};
</script>
