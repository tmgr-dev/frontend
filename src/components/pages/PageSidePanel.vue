<template>
	<aside class="space-y-6 text-sm" data-testid="page-side-panel">
		<slot name="actions" />

		<section v-if="toc.length" data-testid="page-toc">
			<h2
				class="mb-2 text-xs font-semibold uppercase tracking-wide text-ink-subtle"
			>
				Оглавление
			</h2>
			<ul class="space-y-1">
				<li v-for="entry in toc" :key="`${entry.text}-${entry.occurrence}`">
					<button
						type="button"
						class="block w-full truncate text-left text-gray-700 hover:text-blue-600 dark:text-gray-300 dark:hover:text-blue-400"
						@click="$emit('toc', entry)"
					>
						{{ entry.text }}
					</button>
				</li>
			</ul>
		</section>

		<section data-testid="page-backlinks">
			<h2
				class="mb-2 text-xs font-semibold uppercase tracking-wide text-ink-subtle"
			>
				Обратные ссылки
			</h2>
			<ul v-if="backlinks.length" class="space-y-1">
				<li v-for="link in backlinks" :key="link.id">
					<router-link
						:to="`/${workspaceCode}/pages/${link.slug}`"
						class="block truncate text-blue-600 hover:underline dark:text-blue-400"
					>
						{{ link.title }}
					</router-link>
				</li>
			</ul>
			<p v-else class="text-gray-400 dark:text-gray-500">Нет</p>
		</section>

		<section data-testid="page-recent-versions">
			<div class="mb-2 flex items-center justify-between">
				<h2
					class="text-xs font-semibold uppercase tracking-wide text-ink-subtle"
				>
					Версии
				</h2>
				<router-link
					:to="`/${workspaceCode}/pages/${slug}/versions`"
					class="text-xs text-blue-600 hover:underline dark:text-blue-400"
				>
					Все
				</router-link>
			</div>
			<ul v-if="versions.length" class="space-y-2">
				<li v-for="version in versions" :key="version.version">
					<router-link
						:to="`/${workspaceCode}/pages/${slug}/versions?v=${version.version}`"
						class="block rounded p-1 hover:bg-gray-100 dark:hover:bg-gray-800"
					>
						<span class="flex items-center gap-1.5">
							<span class="font-medium text-gray-800 dark:text-gray-100"
								>v{{ version.version }}</span
							>
							<AuthorBadge
								:author="authorOf(version.author)"
								:size="14"
								:show-label="false"
							/>
							<span class="truncate text-xs text-gray-500 dark:text-gray-400">{{
								formatDate(version.created_at)
							}}</span>
						</span>
						<span
							v-if="version.summary"
							class="block truncate text-xs text-gray-500 dark:text-gray-400"
							>{{ version.summary }}</span
						>
					</router-link>
				</li>
			</ul>
			<p v-else class="text-gray-400 dark:text-gray-500">Нет</p>
		</section>

		<section data-testid="page-files-slot">
			<slot name="files" />
		</section>
	</aside>
</template>

<script lang="ts">
	import type {
		PageAuthor,
		PageSummary,
		PageVersion,
	} from '@/actions/tmgr/pages';
	import AuthorBadge from '@/components/general/AuthorBadge.vue';
	import { toAuthorRef } from '@/utils/pages/author';
	import type { TocEntry } from '@/utils/pages/toc';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'PageSidePanel',
		components: { AuthorBadge },
		props: {
			toc: { type: Array as PropType<TocEntry[]>, default: () => [] },
			backlinks: { type: Array as PropType<PageSummary[]>, default: () => [] },
			versions: { type: Array as PropType<PageVersion[]>, default: () => [] },
			workspaceCode: { type: String, required: true },
			slug: { type: String, required: true },
		},
		emits: ['toc'],
		setup() {
			const authorOf = (author: PageAuthor) => toAuthorRef(author);
			const formatDate = (value: string) => new Date(value).toLocaleString();
			return { authorOf, formatDate };
		},
	});
</script>
