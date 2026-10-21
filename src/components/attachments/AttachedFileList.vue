<template>
	<ul v-if="files.length" class="flex flex-wrap gap-1.5">
		<li v-for="file in files" :key="file.id">
			<button
				type="button"
				class="flex max-w-[14rem] items-center gap-1.5 rounded-md border border-line bg-surface px-2 py-1 text-xs text-ink transition hover:bg-surface-hover disabled:opacity-60 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100 dark:hover:bg-gray-700"
				:title="`Download ${file.name}`"
				:disabled="busyId === file.id"
				data-testid="attached-file"
				@click="download(file)"
			>
				<Loader2
					v-if="busyId === file.id"
					class="h-3 w-3 flex-shrink-0 animate-spin text-ink-subtle"
				/>
				<FileText v-else class="h-3 w-3 flex-shrink-0 text-ink-subtle" />
				<span class="min-w-0 truncate">{{ file.name }}</span>
				<span class="flex-shrink-0 text-2xs text-ink-faint dark:text-gray-400">
					{{ kind(file)
					}}<template v-if="file.size != null"> · {{ size(file) }}</template>
				</span>
			</button>
		</li>
	</ul>
</template>

<script lang="ts">
	import { fetchFileObjectUrl } from '@/actions/tmgr/files';
	import type { AttachedFile } from '@/utils/aiAttachments';
	import { attachmentKindLabel, formatBytes } from '@/utils/aiAttachments';
	import { FileText, Loader2 } from 'lucide-vue-next';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'AttachedFileList',
		components: { FileText, Loader2 },
		props: {
			files: {
				type: Array as PropType<AttachedFile[]>,
				default: () => [],
			},
		},
		data() {
			return { busyId: null as number | null };
		},
		methods: {
			kind(file: AttachedFile): string {
				return attachmentKindLabel(file.name, file.mime_type);
			},
			size(file: AttachedFile): string {
				return formatBytes(file.size);
			},
			async download(file: AttachedFile) {
				this.busyId = file.id;
				try {
					const url = await fetchFileObjectUrl(file.id);
					const link = document.createElement('a');
					link.href = url;
					link.download = file.name;
					link.click();
					setTimeout(() => URL.revokeObjectURL(url), 10000);
				} catch {
					return;
				} finally {
					this.busyId = null;
				}
			},
		},
	});
</script>
