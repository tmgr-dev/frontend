<template>
	<div class="task-attachments">
		<div v-if="isLoading" class="py-2 text-center text-sm text-gray-500">
			Loading...
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
					:class="[
						'flex h-16 w-16 cursor-pointer items-center justify-center overflow-hidden rounded-lg border transition-all',
						file.uploading
							? 'border-blue-300 bg-blue-50 dark:border-blue-700 dark:bg-blue-900/20'
							: 'border-gray-200 bg-gray-100 hover:border-gray-400 dark:border-gray-700 dark:bg-gray-800 dark:hover:border-gray-500',
					]"
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
					<component
						v-else
						:is="getFileIcon(file)"
						:size="24"
						:class="getFileIconClass(file)"
					/>
					<!-- Uploading overlay -->
					<div
						v-if="file.uploading"
						class="absolute inset-0 flex items-center justify-center bg-white/50 dark:bg-black/50"
					>
						<div class="h-4 w-4 animate-spin rounded-full border-2 border-blue-500 border-t-transparent"></div>
					</div>
				</div>

				<!-- Actions (on hover) -->
				<div
					v-if="!file.uploading"
					class="absolute -right-1 -top-1 flex gap-0.5 opacity-0 transition-opacity group-hover:opacity-100"
				>
					<button
						v-if="file.id"
						@click.stop="handleDownloadFile(file)"
						class="rounded-full bg-white p-1 shadow-md hover:bg-gray-100 dark:bg-gray-800 dark:hover:bg-gray-700"
						title="Download"
					>
						<Download :size="12" class="text-gray-600 dark:text-gray-400" />
					</button>
					<button
						@click.stop="handleRemoveFile(file)"
						class="rounded-full bg-white p-1 shadow-md hover:bg-red-50 dark:bg-gray-800 dark:hover:bg-red-900/20"
						title="Remove"
					>
						<X :size="12" class="text-red-500" />
					</button>
				</div>

				<!-- File name tooltip on hover -->
				<div
					class="absolute left-1/2 top-full z-10 mt-1 hidden -translate-x-1/2 whitespace-nowrap rounded bg-gray-900 px-2 py-1 text-xs text-white group-hover:block dark:bg-gray-700"
				>
					{{ truncateFileName(file.name, 20) }}
				</div>
			</div>

			<!-- Add more button -->
			<div
				v-if="taskId && !isUploading"
				@click="handleAddFiles"
				class="flex h-16 w-16 cursor-pointer items-center justify-center rounded-lg border-2 border-dashed border-gray-300 text-gray-400 transition-colors hover:border-gray-400 hover:bg-gray-50 hover:text-gray-500 dark:border-gray-600 dark:hover:border-gray-500 dark:hover:bg-gray-800/50"
				title="Add files"
			>
				<Plus :size="24" />
			</div>
		</div>

		<!-- Empty state with drop zone (hidden when hideEmptyState is true) -->
		<template v-else-if="!hideEmptyState">
			<div class="text-sm text-gray-500 dark:text-gray-400">
				No attachments yet
			</div>

			<!-- Drop zone -->
			<div
				v-if="taskId"
				@click="handleAddFiles"
				@dragover.prevent="handleDragOver"
				@dragleave.prevent="handleDragLeave"
				@drop.prevent="handleDrop"
				:class="[
					'mt-2 cursor-pointer rounded-lg border-2 border-dashed p-4 text-center transition-colors',
					isDragOver
						? 'border-blue-500 bg-blue-50 dark:border-blue-400 dark:bg-blue-900/20'
						: 'border-gray-300 hover:border-gray-400 hover:bg-gray-50 dark:border-gray-600 dark:hover:border-gray-500 dark:hover:bg-gray-800/50',
				]"
			>
				<p class="text-sm text-gray-500 dark:text-gray-400">
					{{ isUploading ? 'Uploading...' : 'Click or drag files here' }}
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
			class="fixed inset-0 z-50 flex items-center justify-center bg-black/80"
			@click="closePreview"
		>
			<div
				class="relative max-h-[90vh] max-w-[90vw] overflow-auto rounded-lg bg-white shadow-2xl dark:bg-gray-900"
				@click.stop
			>
				<!-- Modal Header -->
				<div class="sticky top-0 z-10 flex items-center justify-between border-b border-gray-200 bg-white px-4 py-3 dark:border-gray-700 dark:bg-gray-900">
					<h3 class="truncate text-sm font-medium text-gray-900 dark:text-white">
						{{ previewFile.name }}
					</h3>
					<div class="flex items-center gap-2">
						<button
							@click="handleDownloadFile(previewFile)"
							class="rounded p-1.5 text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800"
							title="Download"
						>
							<Download :size="18" />
						</button>
						<button
							@click="closePreview"
							class="rounded p-1.5 text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800"
							title="Close"
						>
							<X :size="18" />
						</button>
					</div>
				</div>

				<!-- Modal Content -->
				<div class="p-4">
					<!-- Loading -->
					<div v-if="isLoadingPreview" class="flex h-64 w-96 items-center justify-center">
						<div class="h-8 w-8 animate-spin rounded-full border-4 border-blue-500 border-t-transparent"></div>
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
						class="max-h-[70vh] max-w-[800px] overflow-auto rounded bg-gray-100 p-4 text-sm dark:bg-gray-800"
					>{{ previewContent }}</pre>

					<!-- Unsupported type -->
					<div v-else class="flex h-64 w-96 flex-col items-center justify-center text-gray-500">
						<FileIcon :size="48" class="mb-4" />
						<p>Preview not available for this file type</p>
						<button
							@click="handleDownloadFile(previewFile)"
							class="mt-4 rounded bg-blue-500 px-4 py-2 text-sm text-white hover:bg-blue-600"
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
	import { FileIcon, X, Download, Plus, Image, Video, FileText, File } from 'lucide-vue-next';
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
		components: {
			FileIcon,
			X,
			Download,
			Plus,
			Image,
			Video,
			FileText,
			File,
		},
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
						const type = getFileType(file.mime_type, file.name);
						if (type === 'image') {
							try {
								file.thumbnailUrl = await getFilePreviewUrl(file.id, file.mime_type);
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
				const type = getFileType(file.mime_type || null, file.name);
				switch (type) {
					case 'image': return 'Image';
					case 'video': return 'Video';
					case 'pdf': return 'FileText';
					case 'text': return 'FileText';
					default: return 'File';
				}
			},
			getFileIconClass(file) {
				const type = getFileType(file.mime_type || null, file.name);
				switch (type) {
					case 'image': return 'text-green-500';
					case 'video': return 'text-purple-500';
					case 'pdf': return 'text-red-500';
					case 'text': return 'text-blue-500';
					default: return 'text-gray-500 dark:text-gray-400';
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

				const type = getFileType(file.mime_type || null, file.name);
				if (!isPreviewable(file.mime_type, file.name)) {
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
						this.previewContent = await getFileTextContent(file.id);
					} else {
						this.previewUrl = await getFilePreviewUrl(file.id, file.mime_type);
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
						mime_type: file.type,
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
					await downloadTaskFile(file.id, file.name);
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

<style lang="scss" scoped>
	.task-attachments {
		width: 100%;
	}
</style>
