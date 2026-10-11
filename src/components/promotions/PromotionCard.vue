<template>
	<a
		:href="promotion.url"
		class="group flex items-start gap-3 rounded-lg border border-border bg-card p-3 text-card-foreground no-underline transition-colors hover:bg-muted"
		:class="compact ? 'text-xs' : 'text-sm'"
		data-testid="promotion-card"
		target="_blank"
		rel="noopener noreferrer"
		@click.prevent="$emit('click', promotion)"
	>
		<img
			v-if="promotion.icon_url"
			:src="promotion.icon_url"
			alt=""
			class="shrink-0 rounded-md object-cover"
			:class="compact ? 'h-8 w-8' : 'h-10 w-10'"
		/>
		<div class="min-w-0 flex-1">
			<div class="flex flex-wrap items-baseline gap-x-2">
				<span class="font-semibold text-foreground">{{
					promotion.product
				}}</span>
				<span class="text-muted-foreground">{{ promotion.title }}</span>
			</div>
			<p v-if="promotion.text" class="mt-0.5 text-muted-foreground">
				{{ promotion.text }}
			</p>
		</div>
		<ExternalLink
			class="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground group-hover:text-foreground"
		/>
	</a>
</template>

<script>
	import { ExternalLink } from 'lucide-vue-next';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'PromotionCard',
		components: { ExternalLink },
		props: {
			promotion: { type: Object, required: true },
			compact: { type: Boolean, default: false },
		},
		emits: ['click'],
	});
</script>
