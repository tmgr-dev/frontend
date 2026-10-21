<template>
	<div v-if="items.length || notice" class="flex flex-col gap-1">
		<ul v-if="items.length" class="flex flex-wrap gap-1.5">
			<li
				v-for="item in items"
				:key="item.key"
				:title="item.error || item.name"
				:class="[
					'relative flex max-w-[14rem] items-center gap-1.5 overflow-hidden rounded-md border px-2 py-1 text-xs',
					item.status === 'error'
						? 'border-red-300 bg-red-50 text-red-700 dark:border-red-800 dark:bg-red-900/20 dark:text-red-300'
						: 'border-line bg-surface text-ink dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100',
				]"
				data-testid="attachment-chip"
			>
				<Loader2
					v-if="item.status === 'uploading'"
					class="h-3 w-3 flex-shrink-0 animate-spin text-ink-subtle"
				/>
				<AlertCircle
					v-else-if="item.status === 'error'"
					class="h-3 w-3 flex-shrink-0"
				/>
				<FileText v-else class="h-3 w-3 flex-shrink-0 text-ink-subtle" />
				<span class="min-w-0 truncate">{{ item.name }}</span>
				<span class="flex-shrink-0 text-2xs text-ink-faint dark:text-gray-400">
					{{ kind(item) }} · {{ size(item) }}
				</span>
				<button
					type="button"
					class="flex-shrink-0 rounded p-0.5 text-ink-subtle hover:bg-surface-hover hover:text-ink dark:hover:bg-gray-700"
					:aria-label="`Remove ${item.name}`"
					@click="$emit('remove', item.key)"
				>
					<X class="h-3 w-3" />
				</button>
				<span
					v-if="item.status === 'uploading'"
					class="absolute bottom-0 left-0 h-0.5 bg-brand transition-all"
					:style="{ width: `${item.progress}%` }"
				/>
			</li>
		</ul>
		<p
			v-if="notice"
			class="text-xs text-red-600 dark:text-red-400"
			role="alert"
		>
			{{ notice }}
		</p>
		<p
			v-for="item in failed"
			:key="`err-${item.key}`"
			class="text-xs text-red-600 dark:text-red-400"
			role="alert"
		>
			{{ item.name }}: {{ item.error }}
		</p>
	</div>
</template>

<script lang="ts">
	import type { AttachmentItem } from '@/composable/useAttachmentQueue';
	import { attachmentKindLabel, formatBytes } from '@/utils/aiAttachments';
	import { AlertCircle, FileText, Loader2, X } from 'lucide-vue-next';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'AttachmentChips',
		components: { AlertCircle, FileText, Loader2, X },
		props: {
			items: {
				type: Array as PropType<AttachmentItem[]>,
				required: true,
			},
			notice: {
				type: String as PropType<string | null>,
				default: null,
			},
		},
		emits: ['remove'],
		computed: {
			failed(): AttachmentItem[] {
				return this.items.filter((i) => i.status === 'error');
			},
		},
		methods: {
			kind(item: AttachmentItem): string {
				return attachmentKindLabel(item.name, item.mimeType);
			},
			size(item: AttachmentItem): string {
				return item.status === 'uploading' && item.progress < 100
					? `${item.progress}%`
					: formatBytes(item.size);
			},
		},
	});
</script>
