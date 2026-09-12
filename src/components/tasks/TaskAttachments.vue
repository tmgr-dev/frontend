<template>
	<div class="task-attachments mt-4">
		<div class="mb-3 flex items-center justify-between">
			<h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300">
				Attachments
				<span v-if="files.length" class="font-normal text-gray-400">
					({{ files.length }})
				</span>
			</h3>
			<button
				type="button"
				@click="handleAddFiles"
				class="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm text-blue-600 transition-colors hover:bg-blue-50 dark:text-blue-400 dark:hover:bg-gray-800"
			>
				<Paperclip :size="16" />
				<span>Add files</span>
			</button>
		</div>

		<div v-if="files.length > 0 || uploads.length > 0" class="mb-3 space-y-2">
			<div
				v-for="file in files"
				:key="file.id"
				class="flex items-center gap-3 rounded-lg border border-gray-200 bg-gray-50 p-3 transition-colors hover:border-gray-300 dark:border-gray-700 dark:bg-gray-800 dark:hover:border-gray-600"
			>
				<div class="flex-shrink-0">
					<img
						v-if="previews[file.id]"
						:src="previews[file.id]"
						:alt="file.name"
						class="h-12 w-12 rounded object-cover"
					/>
					<FileIcon
						v-else
						:size="20"
						class="text-gray-500 dark:text-gray-400"
					/>
				</div>
				<div class="min-w-0 flex-1">
					<p
						class="truncate text-sm font-medium text-gray-900 dark:text-gray-100"
					>
						{{ file.name }}
					</p>
					<p class="text-xs text-gray-500 dark:text-gray-400">
						{{ formatFileSize(file.size) }}
					</p>
				</div>
				<button
					type="button"
					:disabled="busyFileId === file.id"
					@click="download(file)"
					class="flex-shrink-0 rounded p-1.5 text-gray-400 transition-colors hover:bg-gray-200 hover:text-gray-600 disabled:opacity-50 dark:hover:bg-gray-700 dark:hover:text-gray-200"
					title="Download"
				>
					<Download :size="16" />
				</button>
				<button
					type="button"
					:disabled="busyFileId === file.id"
					@click="remove(file)"
					class="flex-shrink-0 rounded p-1.5 text-gray-400 transition-colors hover:bg-red-50 hover:text-red-500 disabled:opacity-50 dark:hover:bg-red-900/20 dark:hover:text-red-400"
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
						? 'border-red-200 bg-red-50 dark:border-red-900/40 dark:bg-red-900/10'
						: 'border-gray-200 bg-gray-50 dark:border-gray-700 dark:bg-gray-800'
				"
			>
				<div class="flex-shrink-0">
					<Loader2
						v-if="!upload.error"
						:size="20"
						class="animate-spin text-blue-500"
					/>
					<AlertCircle v-else :size="20" class="text-red-500" />
				</div>
				<div class="min-w-0 flex-1">
					<p
						class="truncate text-sm font-medium text-gray-900 dark:text-gray-100"
					>
						{{ upload.name }}
					</p>
					<p
						class="text-xs"
						:class="
							upload.error
								? 'text-red-600 dark:text-red-400'
								: 'text-gray-500 dark:text-gray-400'
						"
					>
						{{ upload.error || `Uploading ${formatFileSize(upload.size)}…` }}
					</p>
				</div>
				<button
					v-if="upload.error"
					type="button"
					@click="dismissUpload(upload.id)"
					class="flex-shrink-0 rounded p-1.5 text-gray-400 transition-colors hover:bg-red-100 hover:text-red-500 dark:hover:bg-red-900/20"
				>
					<X :size="16" />
				</button>
			</div>
		</div>

		<div
			@click="handleAddFiles"
			@dragover.prevent="handleDragOver"
			@dragleave.prevent="handleDragLeave"
			@drop.prevent="handleDrop"
			:class="[
				'cursor-pointer rounded-lg border-2 border-dashed p-6 text-center transition-colors',
				isDragOver
					? 'border-blue-500 bg-blue-50 dark:border-blue-400 dark:bg-blue-900/20'
					: 'border-gray-300 hover:border-gray-400 hover:bg-gray-50 dark:border-gray-600 dark:hover:border-gray-500 dark:hover:bg-gray-800/50',
			]"
		>
			<FileIcon
				:size="32"
				class="mx-auto mb-2 text-gray-400 dark:text-gray-500"
			/>
			<p class="text-sm text-gray-500 dark:text-gray-400">
				{{ files.length === 0 ? 'No attachments yet' : 'Add more files' }}
			</p>
			<p class="mt-1 text-xs text-gray-400 dark:text-gray-500">
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
	</div>
</template>

<script lang="ts">
	import {
		AlertCircle,
		Download,
		FileIcon,
		Loader2,
		Paperclip,
		X,
	} from 'lucide-vue-next';
	import { defineComponent } from 'vue';
	import {
		detachFile,
		fetchFileObjectUrl,
		getTaskFiles,
		uploadTaskFile,
		type TaskFile,
	} from '@/actions/tmgr/files';
	import {
		attachmentErrorMessage,
		formatFileSize,
		isImageMime,
		preflightError,
	} from '@/utils/attachments';

	interface PendingUpload {
		id: number;
		name: string;
		size: number;
		error: string | null;
	}

	export default defineComponent({
		name: 'TaskAttachments',
		components: {
			AlertCircle,
			Download,
			FileIcon,
			Loader2,
			Paperclip,
			X,
		},
		emits: ['changed'],
		props: {
			taskId: {
				type: Number,
				required: false,
				default: null,
			},
		},
		data() {
			return {
				files: [] as TaskFile[],
				uploads: [] as PendingUpload[],
				previews: {} as Record<number, string>,
				isDragOver: false,
				busyFileId: null as number | null,
				nextUploadId: 1,
				maxBytes: null as number | null,
			};
		},
		created() {
			this.load();
		},
		methods: {
			formatFileSize,
			async load() {
				if (!this.taskId) {
					return;
				}
				try {
					this.files = await getTaskFiles(this.taskId);
					this.files.forEach((file) => this.loadPreview(file));
				} catch {
					// A task whose files cannot be listed still has to render the rest of the form.
					this.files = [];
				}
			},
			async loadPreview(file: TaskFile) {
				if (!isImageMime(file.mime_type) || this.previews[file.id]) {
					return;
				}
				try {
					this.previews[file.id] = await fetchFileObjectUrl(file.id);
				} catch {
					// No preview is a cosmetic loss; the file is still listed and downloadable.
				}
			},
			handleAddFiles() {
				(this.$refs.fileInput as HTMLInputElement).click();
			},
			handleFileSelect(event: Event) {
				const input = event.target as HTMLInputElement;
				this.uploadAll(Array.from(input.files ?? []));
				input.value = '';
			},
			handleDragOver() {
				this.isDragOver = true;
			},
			handleDragLeave(event: DragEvent) {
				const target = event.currentTarget as Node;
				if (!target.contains(event.relatedTarget as Node)) {
					this.isDragOver = false;
				}
			},
			handleDrop(event: DragEvent) {
				this.isDragOver = false;
				this.uploadAll(Array.from(event.dataTransfer?.files ?? []));
			},
			uploadAll(selected: File[]) {
				selected.forEach((file) => this.upload(file));
			},
			async upload(file: File) {
				if (!this.taskId) {
					return;
				}
				const pending: PendingUpload = {
					id: this.nextUploadId++,
					name: file.name,
					size: file.size,
					error: preflightError(file),
				};
				this.uploads.push(pending);
				if (pending.error) {
					return;
				}

				try {
					const attached = await uploadTaskFile(this.taskId, file);
					this.files.unshift(attached);
					this.loadPreview(attached);
					this.dismissUpload(pending.id);
					this.$emit('changed', this.files.length);
				} catch (error) {
					pending.error = attachmentErrorMessage(error, this.maxBytes);
					this.maxBytes =
						(error as { response?: { data?: { max_bytes?: number } } })?.response
							?.data?.max_bytes ?? this.maxBytes;
				}
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
					URL.revokeObjectURL(url);
					delete this.previews[fileId];
				}
			},
		},
		watch: {
			taskId() {
				this.load();
			},
		},
		unmounted() {
			Object.keys(this.previews).forEach((id) => this.revokePreview(Number(id)));
		},
	});
</script>

<style lang="scss" scoped>
	.task-attachments {
		width: 100%;
	}
</style>
