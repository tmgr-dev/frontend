<template>
	<header
		class="mb-6 flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between"
		data-page-header
	>
		<div class="min-w-0">
			<router-link
				v-if="back"
				:to="back"
				class="mb-2 inline-flex items-center gap-1 text-sm text-ink-subtle hover:text-ink"
			>
				<ChevronLeft class="h-4 w-4" />
				{{ backLabel }}
			</router-link>
			<h1 class="break-words text-xl font-semibold text-ink md:text-2xl">
				<slot name="title">{{ title }}</slot>
			</h1>
			<p
				v-if="subtitle || $slots.subtitle"
				class="mt-1 text-sm text-ink-subtle"
			>
				<slot name="subtitle">{{ subtitle }}</slot>
			</p>
		</div>
		<div
			v-if="$slots.actions"
			class="flex flex-wrap items-center gap-2 sm:shrink-0 sm:justify-end"
		>
			<slot name="actions" />
		</div>
	</header>
</template>

<script>
	import { defineComponent } from 'vue';
	import { ChevronLeft } from 'lucide-vue-next';

	export default defineComponent({
		name: 'PageHeader',
		components: { ChevronLeft },
		props: {
			title: { type: String, default: '' },
			subtitle: { type: String, default: '' },
			back: { type: [String, Object], default: null },
			backLabel: { type: String, default: 'Back' },
		},
	});
</script>
