<template>
	<div class="space-y-6 text-sm">
		<div v-for="section in nav" :key="section.title">
			<p
				class="mb-2 px-2 text-xs font-semibold tracking-wide text-ink-subtle uppercase"
			>
				{{ section.title }}
			</p>
			<ul class="space-y-0.5">
				<li v-for="item in section.items" :key="item.slug">
					<router-link
						:to="docPath(lang, item.slug)"
						:aria-current="item.slug === slug ? 'page' : undefined"
						class="block rounded-md px-2 py-1.5 transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
						:class="
							item.slug === slug
								? 'bg-muted font-medium text-ink'
								: 'text-ink-subtle hover:bg-muted hover:text-ink'
						"
						data-testid="docs-nav-item"
					>
						{{ item.title }}
					</router-link>
				</li>
			</ul>
		</div>
	</div>
</template>

<script lang="ts">
	import { docPath, type DocsLang, type DocsNavSection } from '@/utils/docs';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'DocsNav',
		props: {
			nav: { type: Array as PropType<DocsNavSection[]>, required: true },
			lang: { type: String as PropType<DocsLang>, required: true },
			slug: { type: String, required: true },
		},
		setup() {
			return { docPath };
		},
	});
</script>
