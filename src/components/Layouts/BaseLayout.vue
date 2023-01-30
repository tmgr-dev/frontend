<template>
	<div class="w-screen">
		<header class="container mx-auto px-4 my-5 relative">
			<h1
				class="md:text-3xl text-2xl mt-6 pt-1 md:pt-0 md:mt-0 text-blue-800 dark:text-white relative text-center md:text-left text-center"
			>
				<slot name="header" />
			</h1>

			<slot name="action" />
		</header>

		<div class="container mx-auto px-2 my-5">
			<slot name="body" />
		</div>

		<div class="flex flex-col fixed z-50 right-0 bottom-0 mr-4">
			<slot name="alerts" />
		</div>

		<slot name="footer" />

		<div
			v-if="!noCopyright"
			class="container mx-auto px-5 my-5 text-center text-gray-600 pb-5"
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
