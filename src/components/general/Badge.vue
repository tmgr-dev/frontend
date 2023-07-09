<template>
	<div
		:class="`bg-${color}-100 text-${color}-700`"
		class="flex h-6 items-center rounded-full px-3 text-xs font-semibold"
	>
		<span :class="`bg-${color}-400`" class="mr-1 h-2 w-2 rounded-full"></span>
		<span>
			<slot></slot>
		</span>
	</div>
</template>

<script>
	export default {
		props: {
			color: {
				type: String,
				default: 'teal',
			},
		},
	};
</script>
