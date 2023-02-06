<template>
	<div
		:class="`bg-${color}-100 text-${color}-700`"
		class="px-3 h-6 rounded-full text-xs font-semibold flex items-center"
	>
		<span :class="`bg-${color}-400`" class="w-2 h-2 rounded-full mr-1"></span>
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
