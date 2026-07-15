<template>
	<div class="task-attachments">
		<div v-if="isLoading" class="py-2 text-center text-2xs text-ink-subtle">
			Loading…
		</div>

		<div v-else-if="allFiles.length > 0" class="flex flex-wrap gap-2">
			<div
				v-for="file in allFiles"
				:key="file.id || file.tempId"
				class="group relative"
			>
				<div
					@click="openPreview(file)"
					class="flex h-16 w-16 cursor-pointer items-center justify-center overflow-hidden rounded-card border transition-colors"
					:class="file.uploading ? 'border-line-strong bg-surface-sunken' : 'border-line bg-surface hover:border-line-strong'"
					:title="file.name"
				>
					<img
						v-if="file.thumbnailUrl || file.preview"
						:src="file.thumbnailUrl || file.preview"
						:alt="file.name"
						class="h-full w-full object-cover"
					/>
					<span
						v-else
						class="material-icons"
						:class="getFileIconClass(file)"
						style="font-size: 26px"
						>{{ getFileIcon(file) }}</span
					>
					<div
						v-if="file.uploading"
						class="absolute inset-0 flex items-center justify-center bg-surface/60"
					>
						<div class="h-4 w-4 animate-spin rounded-full border-2 border-brand border-t-transparent"></div>
					</div>
				</div>

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

				<div
					class="pointer-events-none absolute left-1/2 top-full z-10 mt-1 hidden -translate-x-1/2 whitespace-nowrap rounded-md bg-ink px-2 py-1 text-2xs text-surface-base group-hover:block"
				>
					{{ truncateFileName(file.name, 20) }}
				</div>
			</div>

			<div
				v-if="taskId && !isUploading"
				@click="handleAddFiles"
				class="flex h-16 w-16 cursor-pointer items-center justify-center rounded-card border-2 border-dashed border-line-strong text-ink-subtle transition hover:bg-surface-sunken hover:text-ink"
				title="Add files"
			>
				<span class="material-icons" style="font-size: 22px">add</span>
			</div>
		</div>

		<template v-else-if="!hideEmptyState">
			<div class="text-2xs text-ink-subtle">No attachments yet</div>

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

		<div
			v-if="previewFile"
			class="fixed inset-0 z-50 flex items-center justify-center bg-black/70"
			@click="closePreview"
		>
			<div
				class="relative max-h-[90vh] max-w-[90vw] overflow-auto rounded-card border border-line bg-surface shadow-2xl"
				@click.stop
			>
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

				<div class="p-4">
					<div v-if="isLoadingPreview" class="flex h-64 w-96 items-center justify-center">
						<div class="h-8 w-8 animate-spin rounded-full border-4 border-brand border-t-transparent"></div>
					</div>

					<img
						v-else-if="previewType === 'image' && previewUrl"
						:src="previewUrl"
						:alt="previewFile.name"
						class="max-h-[70vh] max-w-full object-contain"
					/>

					<video
						v-else-if="previewType === 'video' && previewUrl"
						:src="previewUrl"
						controls
						class="max-h-[70vh] max-w-full"
					/>

					<iframe
						v-else-if="previewType === 'pdf' && previewUrl"
						:src="previewUrl"
						class="h-[70vh] w-[800px] max-w-full"
					/>

					<pre
						v-else-if="previewType === 'text' && previewContent !== null"
						class="max-h-[70vh] max-w-[800px] overflow-auto rounded-md bg-surface-sunken p-4 text-sm text-ink"
					>{{ previewContent }}</pre>

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
	import { toast } from '@/components/ui/toast/use-toast';
	import {
		getTaskFiles,
		uploadTaskFile,
		deleteTaskFile,
		downloadTaskFile,
		getFileBlob,
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

				// taskId can change on the same component instance (e.g. creating
				// a task assigns form.id after mount); guard against an in-flight
				// load for the previous id resolving after a newer one started.
				const requestedTaskId = this.taskId;
				this.isLoading = true;
				try {
					const files = await getTaskFiles(requestedTaskId);
					await Promise.all(
						files.map(async (file) => {
							const type = getFileType(file.mime_type, file.name);
							if (type === 'image') {
								try {
									// Keep the blob around, not just an object URL for it —
									// openPreview derives its own independent URL from the
									// same blob instead of sharing this one, so this array
									// can always be swapped/revoked without racing an open
									// preview modal.
									file.thumbnailBlob = await getFileBlob(file);
									file.thumbnailUrl = URL.createObjectURL(file.thumbnailBlob);
								} catch (e) {
									console.error('Failed to load thumbnail:', e);
								}
							}
						}),
					);

					if (this.taskId !== requestedTaskId) {
						files.forEach((file) => {
							if (file.thumbnailUrl) {
								URL.revokeObjectURL(file.thumbnailUrl);
							}
						});
						return;
					}

					this.savedFiles.forEach((file) => {
						if (file.thumbnailUrl) {
							URL.revokeObjectURL(file.thumbnailUrl);
						}
					});
					this.savedFiles = files;
					this.$emit('update:count', this.savedFiles.length);
				} catch (error) {
					console.error('Failed to load attachments:', error);
				} finally {
					if (this.taskId === requestedTaskId) {
						this.isLoading = false;
					}
				}
			},
			getFileIcon(file) {
				const type = getFileType(file.mime_type || null, file.name);
				switch (type) {
					case 'image': return 'image';
					case 'video': return 'movie';
					case 'pdf': return 'picture_as_pdf';
					case 'text': return 'description';
					default: return 'insert_drive_file';
				}
			},
			getFileIconClass(file) {
				const type = getFileType(file.mime_type || null, file.name);
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

				const type = getFileType(file.mime_type || null, file.name);
				if (!isPreviewable(file.mime_type, file.name)) {
					await this.handleDownloadFile(file);
					return;
				}

				this.previewFile = file;
				this.previewType = type;
				this.previewUrl = null;
				this.previewContent = null;

				// The grid thumbnail already has this image's blob in memory;
				// derive our own object URL from it instead of re-fetching over
				// the network. A fresh URL (not the grid's) means closePreview
				// can always revoke it without racing loadFiles() replacing the
				// grid's own thumbnailUrl out from under an open preview.
				if (type === 'image' && file.thumbnailBlob) {
					this.previewUrl = URL.createObjectURL(file.thumbnailBlob);
					return;
				}

				this.isLoadingPreview = true;
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
			removePendingFile(tempId) {
				const index = this.pendingFiles.findIndex((f) => f.tempId === tempId);
				if (index !== -1) {
					const [removed] = this.pendingFiles.splice(index, 1);
					if (removed.preview) {
						URL.revokeObjectURL(removed.preview);
					}
				}
			},
			async uploadOneFile(file) {
				const tempId = `temp-${++this.tempIdCounter}`;
				this.pendingFiles.push({
					tempId,
					name: file.name,
					size: file.size,
					mime_type: file.type,
					preview: this.createFilePreview(file),
					uploading: true,
				});

				try {
					await uploadTaskFile(this.taskId, file);
					this.removePendingFile(tempId);
				} catch (error) {
					console.error('Failed to upload file:', error);
					this.removePendingFile(tempId);
					toast({
						title: 'Upload failed',
						description: `"${file.name}" couldn't be attached. Please try again.`,
						variant: 'destructive',
					});
				}
			},
			async uploadFiles(files) {
				if (!this.taskId || files.length === 0) return;

				this.isUploading = true;
				await Promise.allSettled(files.map((file) => this.uploadOneFile(file)));
				this.isUploading = false;
				await this.loadFiles();
			},
			async handleRemoveFile(file) {
				if (file.tempId) {
					this.removePendingFile(file.tempId);
					return;
				}

				if (file.id) {
					try {
						await deleteTaskFile(file.id, this.taskId);
						if (file.thumbnailUrl) {
							URL.revokeObjectURL(file.thumbnailUrl);
						}
						const index = this.savedFiles.findIndex((f) => f.id === file.id);
						if (index !== -1) {
							this.savedFiles.splice(index, 1);
						}
					} catch (error) {
						console.error('Failed to delete file:', error);
						toast({
							title: 'Remove failed',
							description: `"${file.name}" couldn't be removed. Please try again.`,
							variant: 'destructive',
						});
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
					toast({
						title: 'Download failed',
						description: `"${file.name}" couldn't be downloaded. Please try again.`,
						variant: 'destructive',
					});
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
