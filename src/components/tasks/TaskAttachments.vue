<template>
	<div class="task-attachments">
		<div v-if="isLoading" class="py-2 text-center text-2xs text-ink-subtle">
			Loading…
		</div>

		<!-- Grid of thumbnails -->
		<div v-else-if="allFiles.length > 0" class="flex flex-wrap gap-2">
			<div
				v-for="file in allFiles"
				:key="file.id || file.tempId"
				class="group relative"
			>
				<!-- Thumbnail -->
				<div
					@click="openPreview(file)"
					class="flex h-16 w-16 cursor-pointer items-center justify-center overflow-hidden rounded-card border transition-colors"
					:class="file.uploading ? 'border-line-strong bg-surface-sunken' : 'border-line bg-surface hover:border-line-strong'"
					:title="file.name"
				>
					<!-- Image thumbnail -->
					<img
						v-if="file.thumbnailUrl || file.preview"
						:src="file.thumbnailUrl || file.preview"
						:alt="file.name"
						class="h-full w-full object-cover"
					/>
					<!-- File type icon -->
					<span
						v-else
						class="material-icons"
						:class="getFileIconClass(file)"
						style="font-size: 26px"
						>{{ getFileIcon(file) }}</span
					>
					<!-- Uploading overlay -->
					<div
						v-if="file.uploading"
						class="absolute inset-0 flex items-center justify-center bg-surface/60"
					>
						<div class="h-4 w-4 animate-spin rounded-full border-2 border-brand border-t-transparent"></div>
					</div>
				</div>

				<!-- Actions (on hover) -->
				<div
					v-if="!file.uploading"
					class="absolute -right-1 -top-1 flex gap-0.5 opacity-0 transition-opacity group-hover:opacity-100"
				>
					<button
						v-if="file.id"
						type="button"
						@click.stop="handleDownloadFile(file)"
						class="flex size-5 items-center justify-center rounded-pill bg-surface shadow-sm hover:bg-surface-hover"
						title="Download"
					>
						<span class="material-icons text-ink-subtle" style="font-size: 12px">download</span>
					</button>
					<button
						type="button"
						@click.stop="handleRemoveFile(file)"
						class="flex size-5 items-center justify-center rounded-pill bg-surface shadow-sm hover:bg-surface-hover"
						title="Remove"
					>
						<span class="material-icons text-status-fix-fg" style="font-size: 12px">close</span>
					</button>
				</div>

				<!-- File name tooltip on hover -->
				<div
					class="pointer-events-none absolute left-1/2 top-full z-10 mt-1 hidden -translate-x-1/2 whitespace-nowrap rounded-md bg-ink px-2 py-1 text-2xs text-surface-base group-hover:block"
				>
					{{ truncateFileName(file.name, 20) }}
				</div>
			</div>

			<!-- Add more button -->
			<div
				v-if="taskId && !isUploading"
				@click="handleAddFiles"
				class="flex h-16 w-16 cursor-pointer items-center justify-center rounded-card border-2 border-dashed border-line-strong text-ink-subtle transition hover:bg-surface-sunken hover:text-ink"
				title="Add files"
			>
				<span class="material-icons" style="font-size: 22px">add</span>
			</div>
		</div>

		<!-- Empty state with drop zone (hidden when hideEmptyState is true) -->
		<template v-else-if="!hideEmptyState">
			<div class="text-2xs text-ink-subtle">No attachments yet</div>

			<!-- Drop zone -->
			<div
				v-if="taskId"
				@click="handleAddFiles"
				@dragover.prevent="handleDragOver"
				@dragleave.prevent="handleDragLeave"
				@drop.prevent="handleDrop"
				class="mt-2 cursor-pointer rounded-card border-2 border-dashed p-4 text-center transition-colors"
				:class="isDragOver ? 'border-brand bg-brand-bg' : 'border-line-strong hover:bg-surface-sunken'"
			>
				<p class="text-2xs text-ink-subtle">
					{{ isUploading ? 'Uploading…' : 'Click or drag files here' }}
				</p>
			</div>
		</template>

		<input
			ref="fileInput"
			type="file"
			multiple
			class="hidden"
			@change="handleFileSelect"
		/>

		<!-- Preview Modal -->
		<div
			v-if="previewFile"
			class="fixed inset-0 z-50 flex items-center justify-center bg-black/70"
			@click="closePreview"
		>
			<div
				class="relative max-h-[90vh] max-w-[90vw] overflow-auto rounded-card border border-line bg-surface shadow-2xl"
				@click.stop
			>
				<!-- Modal Header -->
				<div class="sticky top-0 z-10 flex items-center justify-between border-b border-line bg-surface px-4 py-3">
					<h3 class="truncate text-sm font-semibold text-ink">
						{{ previewFile.name }}
					</h3>
					<div class="flex items-center gap-2">
						<button
							type="button"
							@click="handleDownloadFile(previewFile)"
							class="rounded-md p-1.5 text-ink-subtle hover:bg-surface-hover"
							title="Download"
						>
							<span class="material-icons" style="font-size: 18px">download</span>
						</button>
						<button
							type="button"
							@click="closePreview"
							class="rounded-md p-1.5 text-ink-subtle hover:bg-surface-hover"
							title="Close"
						>
							<span class="material-icons" style="font-size: 18px">close</span>
						</button>
					</div>
				</div>

				<!-- Modal Content -->
				<div class="p-4">
					<!-- Loading -->
					<div v-if="isLoadingPreview" class="flex h-64 w-96 items-center justify-center">
						<div class="h-8 w-8 animate-spin rounded-full border-4 border-brand border-t-transparent"></div>
					</div>

					<!-- Image Preview -->
					<img
						v-else-if="previewType === 'image' && previewUrl"
						:src="previewUrl"
						:alt="previewFile.name"
						class="max-h-[70vh] max-w-full object-contain"
					/>

					<!-- Video Preview -->
					<video
						v-else-if="previewType === 'video' && previewUrl"
						:src="previewUrl"
						controls
						class="max-h-[70vh] max-w-full"
					/>

					<!-- PDF Preview -->
					<iframe
						v-else-if="previewType === 'pdf' && previewUrl"
						:src="previewUrl"
						class="h-[70vh] w-[800px] max-w-full"
					/>

					<!-- Text Preview -->
					<pre
						v-else-if="previewType === 'text' && previewContent"
						class="max-h-[70vh] max-w-[800px] overflow-auto rounded-md bg-surface-sunken p-4 text-sm text-ink"
					>{{ previewContent }}</pre>

					<!-- Unsupported type -->
					<div v-else class="flex h-64 w-96 flex-col items-center justify-center text-ink-subtle">
						<span class="material-icons mb-4" style="font-size: 48px">insert_drive_file</span>
						<p class="text-sm">Preview not available for this file type</p>
						<button
							type="button"
							@click="handleDownloadFile(previewFile)"
							class="mt-4 rounded-pill bg-brand px-4 py-2 text-sm font-semibold text-white hover:bg-brand-hover"
						>
							Download file
						</button>
					</div>
				</div>
			</div>
		</div>
	</div>
</template>

<script>
	import { defineComponent } from 'vue';
	import {
		getTaskFiles,
		uploadTaskFile,
		deleteTaskFile,
		downloadTaskFile,
		getFilePreviewUrl,
		getFileTextContent,
		getFileType,
		isPreviewable,
	} from '@/actions/tmgr/files';

	export default defineComponent({
		name: 'TaskAttachments',
		props: {
			taskId: {
				type: Number,
				required: false,
				default: null,
			},
			hideEmptyState: {
				type: Boolean,
				default: false,
			},
		},
		emits: ['update:count'],
		expose: ['handleAddFiles'],
		data() {
			return {
				savedFiles: [],
				pendingFiles: [],
				isDragOver: false,
				isLoading: false,
				isUploading: false,
				tempIdCounter: 0,
				previewFile: null,
				previewUrl: null,
				previewContent: null,
				previewType: null,
				isLoadingPreview: false,
			};
		},
		computed: {
			allFiles() {
				return [...this.savedFiles, ...this.pendingFiles];
			},
		},
		watch: {
			taskId: {
				immediate: true,
				handler(newTaskId) {
					if (newTaskId) {
						this.loadFiles();
					} else {
						this.savedFiles = [];
						this.$emit('update:count', 0);
					}
				},
			},
			'savedFiles.length': {
				handler(newCount) {
					this.$emit('update:count', newCount);
				},
			},
		},
		methods: {
			async loadFiles() {
				if (!this.taskId) return;

				this.isLoading = true;
				try {
					const files = await getTaskFiles(this.taskId);
					for (const file of files) {
						const type = getFileType(file.mimeType, file.name);
						if (type === 'image') {
							try {
								file.thumbnailUrl = await getFilePreviewUrl(file);
							} catch (e) {
								console.error('Failed to load thumbnail:', e);
							}
						}
					}
					this.savedFiles = files;
					this.$emit('update:count', this.savedFiles.length);
				} catch (error) {
					console.error('Failed to load attachments:', error);
				} finally {
					this.isLoading = false;
				}
			},
			getFileIcon(file) {
				const type = getFileType(file.mimeType || null, file.name);
				switch (type) {
					case 'image': return 'image';
					case 'video': return 'movie';
					case 'pdf': return 'picture_as_pdf';
					case 'text': return 'description';
					default: return 'insert_drive_file';
				}
			},
			getFileIconClass(file) {
				const type = getFileType(file.mimeType || null, file.name);
				switch (type) {
					case 'image': return 'text-status-done';
					case 'video': return 'text-status-testing';
					case 'pdf': return 'text-status-fix-fg';
					case 'text': return 'text-brand-fg';
					default: return 'text-ink-subtle';
				}
			},
			truncateFileName(name, maxLength) {
				if (!name || name.length <= maxLength) return name;
				const ext = name.split('.').pop();
				const nameWithoutExt = name.slice(0, -(ext.length + 1));
				const truncated = nameWithoutExt.slice(0, maxLength - ext.length - 4);
				return `${truncated}...${ext}`;
			},
			async openPreview(file) {
				if (file.uploading || !file.id) return;

				const type = getFileType(file.mimeType || null, file.name);
				if (!isPreviewable(file.mimeType, file.name)) {
					await this.handleDownloadFile(file);
					return;
				}

				this.previewFile = file;
				this.previewType = type;
				this.isLoadingPreview = true;
				this.previewUrl = null;
				this.previewContent = null;

				try {
					if (type === 'text') {
						this.previewContent = await getFileTextContent(file);
					} else {
						this.previewUrl = await getFilePreviewUrl(file);
					}
				} catch (error) {
					console.error('Failed to load preview:', error);
				} finally {
					this.isLoadingPreview = false;
				}
			},
			closePreview() {
				if (this.previewUrl) {
					URL.revokeObjectURL(this.previewUrl);
				}
				this.previewFile = null;
				this.previewUrl = null;
				this.previewContent = null;
				this.previewType = null;
			},
			handleAddFiles() {
				if (!this.taskId || this.isUploading) return;
				this.$refs.fileInput.click();
			},
			isImageFile(file) {
				return file.type?.startsWith('image/');
			},
			createFilePreview(file) {
				if (this.isImageFile(file)) {
					return URL.createObjectURL(file);
				}
				return null;
			},
			async handleFileSelect(event) {
				const selectedFiles = Array.from(event.target.files);
				await this.uploadFiles(selectedFiles);
				event.target.value = '';
			},
			async uploadFiles(files) {
				if (!this.taskId || files.length === 0) return;

				this.isUploading = true;

				for (const file of files) {
					const tempId = `temp-${++this.tempIdCounter}`;
					const pendingFile = {
						tempId,
						name: file.name,
						size: file.size,
						mimeType: file.type,
						preview: this.createFilePreview(file),
						uploading: true,
					};

					this.pendingFiles.push(pendingFile);

					try {
						await uploadTaskFile(this.taskId, file);
						if (pendingFile.preview) {
							URL.revokeObjectURL(pendingFile.preview);
						}
						const index = this.pendingFiles.findIndex(
							(f) => f.tempId === tempId,
						);
						if (index !== -1) {
							this.pendingFiles.splice(index, 1);
						}
					} catch (error) {
						console.error('Failed to upload file:', error);
						const index = this.pendingFiles.findIndex(
							(f) => f.tempId === tempId,
						);
						if (index !== -1) {
							this.pendingFiles.splice(index, 1);
						}
					}
				}

				this.isUploading = false;
				await this.loadFiles();
			},
			async handleRemoveFile(file) {
				if (file.tempId) {
					if (file.preview) {
						URL.revokeObjectURL(file.preview);
					}
					const index = this.pendingFiles.findIndex(
						(f) => f.tempId === file.tempId,
					);
					if (index !== -1) {
						this.pendingFiles.splice(index, 1);
					}
					return;
				}

				if (file.id) {
					try {
						if (file.thumbnailUrl) {
							URL.revokeObjectURL(file.thumbnailUrl);
						}
						await deleteTaskFile(file.id, this.taskId);
						const index = this.savedFiles.findIndex((f) => f.id === file.id);
						if (index !== -1) {
							this.savedFiles.splice(index, 1);
						}
					} catch (error) {
						console.error('Failed to delete file:', error);
					}
				}
			},
			handleDragOver(event) {
				event.preventDefault();
				this.isDragOver = true;
			},
			handleDragLeave(event) {
				event.preventDefault();
				if (!event.currentTarget.contains(event.relatedTarget)) {
					this.isDragOver = false;
				}
			},
			async handleDrop(event) {
				event.preventDefault();
				this.isDragOver = false;
				const droppedFiles = Array.from(event.dataTransfer.files);
				if (droppedFiles.length > 0) {
					await this.uploadFiles(droppedFiles);
				}
			},
			async handleDownloadFile(file) {
				try {
					await downloadTaskFile(file, file.name);
				} catch (error) {
					console.error('Failed to download file:', error);
				}
			},
		},
		unmounted() {
			this.pendingFiles.forEach((file) => {
				if (file.preview) {
					URL.revokeObjectURL(file.preview);
				}
			});
			this.savedFiles.forEach((file) => {
				if (file.thumbnailUrl) {
					URL.revokeObjectURL(file.thumbnailUrl);
				}
			});
			if (this.previewUrl) {
				URL.revokeObjectURL(this.previewUrl);
			}
		},
	});
</script>
