<template>
	<div class="task-attachments mt-4">
		<div v-if="loadError" role="alert" class="py-2 text-sm">
			Could not load attachments.
			<button class="underline" @click="load">Retry</button>
		</div>
		<div class="mb-3 flex items-center justify-between">
			<h3 class="text-sm font-semibold text-ink-muted">
				Attachments
				<span v-if="files.length" class="font-normal text-ink-subtle">
					({{ files.length }})
				</span>
			</h3>
			<button
				type="button"
				@click="handleAddFiles"
				class="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm text-brand transition-colors hover:bg-brand-bg"
			>
				<Paperclip :size="16" />
				<span>Add files</span>
			</button>
		</div>

		<AttachmentsSkeleton v-if="loadingFiles && !files.length && !loadError" />
		<div
			v-if="files.length > 0 || uploads.length > 0 || pendingFiles.length > 0"
			class="space-y-2 mb-3"
		>
			<div
				v-for="(file, index) in pendingFiles"
				:key="`pending-${index}-${file.name}`"
				class="flex items-center gap-3 rounded-lg border border-dashed border-line-strong bg-surface-sunken p-3"
			>
				<div class="flex h-12 w-12 shrink-0 items-center justify-center">
					<FileIcon :size="20" class="text-ink-subtle" />
				</div>
				<div class="min-w-0 flex-1">
					<p class="truncate text-sm font-medium text-ink">
						{{ file.name }}
					</p>
					<p class="text-xs text-ink-subtle">
						{{ formatFileSize(file.size) }} · Will upload when the task is
						created
					</p>
				</div>
				<button
					type="button"
					@click="removePending(index)"
					class="shrink-0 rounded p-1.5 text-ink-subtle transition-colors hover:bg-status-fix-bg hover:text-status-fix-fg"
					title="Remove"
				>
					<X :size="16" />
				</button>
			</div>

			<div
				v-for="file in files"
				:key="file.id"
				:ref="(element) => observePreview(file, element)"
				class="flex items-center gap-3 rounded-lg border border-line bg-surface-sunken p-3 transition-colors hover:border-line-strong"
			>
				<div class="flex h-12 w-12 shrink-0 items-center justify-center">
					<button
						v-if="previews[file.id]"
						type="button"
						class="block cursor-zoom-in rounded"
						:title="`Open ${file.name}`"
						@click="openGallery(file)"
					>
						<img
							:src="previews[file.id]"
							:alt="file.name"
							width="48"
							height="48"
							loading="lazy"
							decoding="async"
							class="h-12 w-12 rounded object-cover"
						/>
					</button>
					<FileIcon v-else :size="20" class="text-ink-subtle" />
				</div>
				<div class="min-w-0 flex-1">
					<p class="truncate text-sm font-medium text-ink">
						{{ file.name }}
					</p>
					<p class="text-xs text-ink-subtle">
						{{ formatFileSize(file.size) }}
					</p>
				</div>
				<button
					type="button"
					:disabled="busyFileId === file.id"
					@click="download(file)"
					class="shrink-0 rounded p-1.5 text-ink-subtle transition-colors hover:bg-surface-active hover:text-ink disabled:opacity-50"
					title="Download"
				>
					<Download :size="16" />
				</button>
				<button
					type="button"
					:disabled="busyFileId === file.id"
					@click="remove(file)"
					class="shrink-0 rounded p-1.5 text-ink-subtle transition-colors hover:bg-status-fix-bg hover:text-status-fix-fg disabled:opacity-50"
					title="Remove"
				>
					<X :size="16" />
				</button>
			</div>

			<div
				v-for="upload in uploads"
				:key="upload.id"
				class="flex items-center gap-3 rounded-lg border p-3"
				:class="
					upload.error
						? 'border-status-fix bg-status-fix-bg'
						: 'border-line bg-surface-sunken'
				"
			>
				<div class="shrink-0">
					<Loader2
						v-if="!upload.error"
						:size="20"
						class="animate-spin text-brand"
					/>
					<AlertCircle v-else :size="20" class="text-status-fix" />
				</div>
				<div class="min-w-0 flex-1">
					<p class="truncate text-sm font-medium text-ink">
						{{ upload.name }}
					</p>
					<p
						class="text-xs"
						:class="upload.error ? 'text-status-fix-fg' : 'text-ink-subtle'"
					>
						{{ upload.error || `Uploading ${formatFileSize(upload.size)}…` }}
					</p>
				</div>
				<button
					v-if="upload.error"
					type="button"
					@click="dismissUpload(upload.id)"
					aria-label="Dismiss upload"
					class="shrink-0 rounded p-1.5 text-ink-subtle transition-colors hover:bg-status-fix-bg hover:text-status-fix-fg"
				>
					<X :size="16" />
				</button>
			</div>
		</div>

		<div
			@click="handleAddFiles"
			class="cursor-pointer rounded-lg border-2 border-dashed border-line-strong p-6 text-center transition-colors hover:bg-surface-hover"
		>
			<FileIcon :size="32" class="mx-auto mb-2 text-ink-subtle" />
			<p class="text-sm text-ink-subtle">
				{{ files.length === 0 ? 'No attachments yet' : 'Add more files' }}
			</p>
			<p class="mt-1 text-xs text-ink-subtle">
				Click or drag files here to attach documents, images, or other files
			</p>
		</div>

		<input
			ref="fileInput"
			type="file"
			multiple
			class="hidden"
			@change="handleFileSelect"
		/>

		<AttachmentGallery
			:images="images"
			:start-id="galleryStartId"
			:urls="previews"
			@close="galleryStartId = null"
		/>
	</div>
</template>

<script lang="ts">
	import {
		detachFile,
		fetchFileObjectUrl,
		fileDisplayUrl,
		getTaskFiles,
		releaseFileDisplayUrl,
		uploadTaskFile,
		type TaskFile,
	} from '@/actions/tmgr/files';
	import AttachmentGallery from '@/components/tasks/AttachmentGallery.vue';
	import AttachmentsSkeleton from '@/components/tasks/AttachmentsSkeleton.vue';
	import { hasActiveLocalWorkspace } from '@/local/runtime';
	import {
		attachmentErrorMessage,
		formatFileSize,
		isImageMime,
		preflightError,
	} from '@/utils/attachments';
	import {
		clipboardImageName,
		imagesFromClipboard,
	} from '@/utils/clipboardImages';
	import { galleryImages } from '@/utils/galleryNavigation';
	import { createVisiblePreviewQueue } from '@/utils/visiblePreviewQueue';
	import {
		AlertCircle,
		Download,
		FileIcon,
		Loader2,
		Paperclip,
		X,
	} from 'lucide-vue-next';
	import {
		defineComponent,
		markRaw,
		type ComponentPublicInstance,
		type PropType,
	} from 'vue';

	interface PendingUpload {
		id: number;
		name: string;
		size: number;
		error: string | null;
	}

	export default defineComponent({
		name: 'TaskAttachments',
		components: {
			AttachmentsSkeleton,
			AttachmentGallery,
			AlertCircle,
			Download,
			FileIcon,
			Loader2,
			Paperclip,
			X,
		},
		emits: ['changed', 'update:pendingFiles'],
		props: {
			taskId: {
				type: Number,
				required: false,
				default: null,
			},
			pendingFiles: {
				type: Array as PropType<File[]>,
				default: () => [],
			},
		},
		data() {
			return {
				disposed: false,
				loadVersion: 0,
				previewVersion: 0,
				previewQueue: markRaw(createVisiblePreviewQueue()),
				loadError: false,
				loadingFiles: true,
				files: [] as TaskFile[],
				uploads: [] as PendingUpload[],
				previews: {} as Record<number, string>,
				busyFileId: null as number | null,
				galleryStartId: null as number | null,
				nextUploadId: 1,
				maxBytes: null as number | null,
			};
		},
		created() {
			this.load();
		},
		computed: {
			images(): TaskFile[] {
				return galleryImages(this.files);
			},
		},
		methods: {
			formatFileSize,
			async load() {
				const version = ++this.loadVersion;
				this.previewQueue.reset();
				this.loadError = false;
				this.loadingFiles = true;
				if (!this.taskId) {
					this.loadingFiles = false;
					return;
				}
				try {
					const files = await getTaskFiles(this.taskId);
					if (this.disposed || version !== this.loadVersion) return;
					Object.keys(this.previews).forEach((id) =>
						this.revokePreview(Number(id)),
					);
					this.files = files;
					this.previewVersion = version;
				} catch {
					if (version === this.loadVersion) this.loadError = true;
				} finally {
					if (version === this.loadVersion) this.loadingFiles = false;
				}
			},
			observePreview(
				file: TaskFile,
				element: Element | ComponentPublicInstance | null,
			) {
				if (!element) {
					this.previewQueue.bind(file.id, null, async () => {});
					return;
				}
				if (
					!(element instanceof Element) ||
					!isImageMime(file.mime_type) ||
					this.previewVersion !== this.loadVersion
				)
					return;
				const version = this.previewVersion;
				this.previewQueue.bind(file.id, element, () =>
					this.loadPreview(file, version),
				);
			},
			async loadPreview(file: TaskFile, requestedVersion?: number) {
				const version = requestedVersion ?? this.loadVersion;
				if (this.disposed || version !== this.loadVersion) return;
				if (!isImageMime(file.mime_type) || this.previews[file.id]) {
					return;
				}
				try {
					const url = await fileDisplayUrl(file.id, { thumb: true });
					if (this.disposed || version !== this.loadVersion) {
						releaseFileDisplayUrl(url);
						return;
					}
					this.previews[file.id] = url;
				} catch {
					// No preview is a cosmetic loss; the file is still listed and downloadable.
				}
			},
			openGallery(file: TaskFile) {
				this.galleryStartId = file.id;
			},
			handleAddFiles() {
				(this.$refs.fileInput as HTMLInputElement).click();
			},
			handleFileSelect(event: Event) {
				const input = event.target as HTMLInputElement;
				this.uploadAll(Array.from(input.files ?? []));
				input.value = '';
			},
			// TM-233: a screenshot in the clipboard becomes an attachment wherever the task has
			// focus. A text paste carries no image, so typing into a field is unaffected; an editor
			// that handles images itself calls preventDefault before this listener sees the event.
			handlePaste(event: ClipboardEvent) {
				if (event.defaultPrevented) {
					return;
				}

				const images = imagesFromClipboard(event.clipboardData?.items);

				if (!images.length) {
					return;
				}

				event.preventDefault();
				this.uploadAll(
					images.map(
						(image) =>
							new File([image], clipboardImageName(image.type), {
								type: image.type,
							}),
					),
				);
			},
			uploadAll(selected: File[]) {
				if (!this.taskId) {
					this.queueAll(selected);
					return;
				}
				selected.forEach((file) => this.upload(file));
			},
			queueAll(selected: File[]) {
				const accepted = selected.filter((file) => {
					const error = preflightError(file, {
						anyType: hasActiveLocalWorkspace(),
					});
					if (error) {
						this.uploads.push({
							id: this.nextUploadId++,
							name: file.name,
							size: file.size,
							error,
						});
					}
					return !error;
				});
				if (accepted.length) {
					this.$emit('update:pendingFiles', [
						...this.pendingFiles,
						...accepted,
					]);
				}
			},
			async upload(file: File) {
				if (!this.taskId) {
					return;
				}
				const pending: PendingUpload = {
					id: this.nextUploadId++,
					name: file.name,
					size: file.size,
					error: preflightError(file, { anyType: hasActiveLocalWorkspace() }),
				};
				this.uploads.push(pending);
				if (pending.error) {
					return;
				}

				try {
					const attached = await uploadTaskFile(this.taskId, file);
					this.files.unshift(attached);

					this.dismissUpload(pending.id);
					this.$emit('changed', this.files.length);
				} catch (error) {
					pending.error = attachmentErrorMessage(error, this.maxBytes);
					this.maxBytes =
						(error as { response?: { data?: { max_bytes?: number } } })
							?.response?.data?.max_bytes ?? this.maxBytes;
				}
			},
			removePending(index: number) {
				this.$emit(
					'update:pendingFiles',
					this.pendingFiles.filter((_, i) => i !== index),
				);
			},
			dismissUpload(id: number) {
				this.uploads = this.uploads.filter((upload) => upload.id !== id);
			},
			async download(file: TaskFile) {
				this.busyFileId = file.id;
				try {
					const url = await fetchFileObjectUrl(file.id);
					const link = document.createElement('a');
					link.href = url;
					link.download = file.name;
					link.click();
					// Revoking straight away cancels the download in some browsers.
					setTimeout(() => URL.revokeObjectURL(url), 10000);
				} catch {
					// Nothing to do beyond leaving the row as it was.
				} finally {
					this.busyFileId = null;
				}
			},
			async remove(file: TaskFile) {
				this.busyFileId = file.id;
				try {
					await detachFile(file.id);
					if (this.galleryStartId === file.id) {
						this.galleryStartId = null;
					}
					this.files = this.files.filter((f) => f.id !== file.id);
					this.revokePreview(file.id);
					this.$emit('changed', this.files.length);
				} catch {
					// Leave the row in place: the file is still attached.
				} finally {
					this.busyFileId = null;
				}
			},
			revokePreview(fileId: number) {
				const url = this.previews[fileId];
				if (url) {
					releaseFileDisplayUrl(url);
					delete this.previews[fileId];
				}
			},
		},
		watch: {
			taskId() {
				this.load();
			},
		},
		mounted() {
			document.addEventListener('paste', this.handlePaste);
		},
		unmounted() {
			this.disposed = true;
			this.previewQueue.dispose();
			this.loadVersion++;
			document.removeEventListener('paste', this.handlePaste);
			Object.keys(this.previews).forEach((id) =>
				this.revokePreview(Number(id)),
			);
		},
	});
</script>

<style lang="scss" scoped>
	.task-attachments {
		width: 100%;
	}
</style>
