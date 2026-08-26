<template>
	<TooltipProvider v-if="visible">
		<Tooltip>
			<TooltipTrigger as-child>
				<slot />
			</TooltipTrigger>
			<TooltipContent :side="side" :side-offset="sideOffset">
				<slot name="content">{{ content }}</slot>
			</TooltipContent>
		</Tooltip>
	</TooltipProvider>
	<slot v-else />
</template>

<script>
	import {
		Tooltip,
		TooltipContent,
		TooltipProvider,
		TooltipTrigger,
	} from '@/components/ui/tooltip';

	export default {
		name: 'AppTooltip',
		components: { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger },
		props: {
			content: {
				type: String,
				default: '',
			},
			side: {
				type: String,
				default: 'top',
			},
			sideOffset: {
				type: Number,
				default: 16,
			},
			disabled: {
				type: Boolean,
				default: false,
			},
		},
		computed: {
			userSettings() {
				return this.$store.state.userSettings || {};
			},
			visible() {
				return (
					!this.disabled &&
					this.userSettings.showTooltips &&
					!!(this.content || this.$slots.content)
				);
			},
		},
	};
</script>
