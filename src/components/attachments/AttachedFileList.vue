<template>
	<div v-if="files.length">
		<ul class="flex flex-wrap items-start gap-1.5">
			<li v-for="file in files" :key="file.id">
				<button
					v-if="isImage(file)"
					:ref="(el) => bindPreview(file, el)"
					type="button"
					class="flex h-24 w-24 items-center justify-center overflow-hidden rounded-md border border-line bg-surface-sunken text-ink-subtle transition hover:border-line-strong"
					:title="`Open ${file.name}`"
					data-testid="attached-file"
					@click="openViewer(file)"
				>
					<img
						v-if="previews[file.id]"
						:src="previews[file.id]"
						:alt="file.name"
						draggable="false"
						class="h-full w-full object-cover"
					/>
					<ImageIcon
						v-else
						class="h-6 w-6"
						:aria-label="file.name"
						role="img"
					/>
				</button>
				<button
					v-else
					type="button"
					class="flex max-w-[14rem] items-center gap-1.5 rounded-md border border-line-strong bg-surface px-2 py-1 text-xs text-ink transition hover:bg-surface-hover"
					:title="`Open ${file.name}`"
					data-testid="attached-file"
					@click="openViewer(file)"
				>
					<FileText class="h-3 w-3 flex-shrink-0 text-ink-subtle" />
					<span class="min-w-0 truncate">{{ file.name }}</span>
					<span class="flex-shrink-0 text-2xs text-ink-subtle">
						{{ kind(file)
						}}<template v-if="file.size != null"> · {{ size(file) }}</template>
					</span>
				</button>
			</li>
		</ul>
		<AttachmentGallery
			v-if="viewerId !== null"
			:images="files"
			:start-id="viewerId"
			:urls="previews"
			@close="viewerId = null"
		/>
	</div>
</template>

<script lang="ts">
	import { fileDisplayUrl, releaseFileDisplayUrl } from '@/actions/tmgr/files';
	import AttachmentGallery from '@/components/tasks/AttachmentGallery.vue';
	import type { AttachedFile } from '@/utils/aiAttachments';
	import { attachmentKindLabel, formatBytes } from '@/utils/aiAttachments';
	import { isImageMime } from '@/utils/attachments';
	import { createVisiblePreviewQueue } from '@/utils/visiblePreviewQueue';
	import { FileText, Image as ImageIcon } from 'lucide-vue-next';
	import {
		type ComponentPublicInstance,
		defineComponent,
		markRaw,
		type PropType,
	} from 'vue';

	export default defineComponent({
		name: 'AttachedFileList',
		components: { AttachmentGallery, FileText, ImageIcon },
		props: {
			files: {
				type: Array as PropType<AttachedFile[]>,
				default: () => [],
			},
		},
		data() {
			return {
				viewerId: null as number | null,
				previews: {} as Record<number, string>,
				previewQueue: markRaw(createVisiblePreviewQueue()),
				disposed: false,
			};
		},
		watch: {
			files() {
				const ids = new Set(this.files.map((file) => file.id));
				Object.keys(this.previews).forEach((key) => {
					const id = Number(key);
					if (!ids.has(id)) {
						releaseFileDisplayUrl(this.previews[id]);
						delete this.previews[id];
					}
				});
			},
		},
		beforeUnmount() {
			this.disposed = true;
			this.previewQueue.dispose();
			Object.values(this.previews).forEach(releaseFileDisplayUrl);
		},
		methods: {
			isImage(file: AttachedFile): boolean {
				return isImageMime(file.mime_type);
			},
			kind(file: AttachedFile): string {
				return attachmentKindLabel(file.name, file.mime_type);
			},
			size(file: AttachedFile): string {
				return formatBytes(file.size);
			},
			openViewer(file: AttachedFile) {
				this.viewerId = file.id;
			},
			bindPreview(
				file: AttachedFile,
				element: Element | ComponentPublicInstance | null,
			) {
				if (!element) {
					this.previewQueue.bind(file.id, null, async () => {});
					return;
				}
				if (element instanceof Element) {
					this.previewQueue.bind(file.id, element, () =>
						this.loadPreview(file),
					);
				}
			},
			async loadPreview(file: AttachedFile) {
				if (this.disposed || this.previews[file.id]) return;
				try {
					const url = await fileDisplayUrl(file.id, { thumb: true });
					if (this.disposed) {
						releaseFileDisplayUrl(url);
						return;
					}
					this.previews[file.id] = url;
				} catch {
					// No thumbnail is a cosmetic loss; the file still opens in the viewer.
				}
			},
		},
	});
</script>
