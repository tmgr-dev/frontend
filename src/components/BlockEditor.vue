<template>
	<div>
		<div class="editorjs" ref="editorRef"></div>
	</div>
</template>

<script setup>
	import EditorJS from '@editorjs/editorjs';
	import Header from '@editorjs/header';
	import List from '@editorjs/list';
	import Checklist from '@editorjs/checklist';
	import Quote from '@editorjs/quote';
	import Warning from '@editorjs/warning';
	import Marker from '@editorjs/marker';
	import CodeTool from '@bomdi/codebox';
	import Delimiter from '@editorjs/delimiter';
	import InlineCode from '@editorjs/inline-code';
	import Table from '@editorjs/table';
	import Raw from '@editorjs/raw';
	import Embed from '@editorjs/embed';
	import { onMounted, onUnmounted, ref, watch } from 'vue';
	import DragDrop from 'editorjs-drag-drop';

	const editorRef = ref(null);
	const props = defineProps(['modelValue', 'placeholder']);
	const emit = defineEmits(['update:modelValue']);
	let editor;
	let updatingModel = false;

	// model -> view
	function modelToView() {
		new DragDrop(editor);
		if (!props.modelValue) {
			return;
		}
		if (typeof props.modelValue === 'string') {
			editor.blocks.renderFromHTML(props.modelValue);
			return;
		}
		editor.render(props.modelValue);
	}

	// view -> model
	function viewToModel(api, event) {
		updatingModel = true;
		editor
			.save()
			.then((outputData) => {
				emit('update:modelValue', outputData);
			})
			.catch((error) => {
				console.log(event, 'Saving failed: ', error);
			})
			.finally(() => {
				updatingModel = false;
			});
	}

	watch(
		() => props.modelValue,
		() => {
			if (!updatingModel) {
				modelToView();
			}
		},
	);

	onMounted(() => {
		editor = new EditorJS({
			holder: editorRef.value,
			placeholder: props.placeholder,
			inlineToolbar: ['bold', 'italic', 'link'],
			tools: {
				header: {
					class: Header,
					config: {
						placeholder: 'Enter a header',
						levels: [1, 2, 3],
						defaultLevel: 1,
					},
				},
				list: {
					class: List,
					inlineToolbar: true,
					config: {
						defaultStyle: 'unordered',
					},
				},
				checklist: {
					class: Checklist,
					inlineToolbar: true,
				},
				quote: {
					class: Quote,
					inlineToolbar: true,
					shortcut: 'CMD+SHIFT+O',
					config: {
						quotePlaceholder: 'Enter a quote',
						captionPlaceholder: "Quote's author",
					},
				},
				warning: {
					class: Warning,
					inlineToolbar: true,
					shortcut: 'CMD+SHIFT+W',
					config: {
						titlePlaceholder: 'Title',
						messagePlaceholder: 'Message',
					},
				},
				marker: {
					class: Marker,
					shortcut: 'CMD+SHIFT+M',
				},
				codebox: {
					class: CodeTool,
					shortcut: 'CMD+SHIFT+C',
					config: {
						themeURL:
							'https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@9.18.1/build/styles/dracula.min.css', // Optional
						themeName: 'atom-one-dark', // Optional
						useDefaultTheme: 'light', // Optional. This also determines the background color of the language select drop-down
					},
				},
				delimiter: Delimiter,
				inlineCode: {
					class: InlineCode,
					shortcut: 'CMD+SHIFT+C',
				},
				table: {
					class: Table,
					inlineToolbar: true,
					config: {
						rows: 2,
						cols: 3,
					},
				},
				raw: Raw,
				// image: {
				// 	class: ImageTool,
				// 	config: {
				// 		uploader: {
				// 			uploadByFile: uploadImage
				// 		},
				// 		captionPlaceholder: 'Image caption'
				// 	}
				// },
				embed: {
					class: Embed,
					config: {
						services: {
							youtube: true,
							codesandbox: true,
							codepen: true,
							figma: true,
						},
					},
				},
			},
			minHeight: 'auto',
			data: props.modelValue,
			onReady: modelToView,
			onChange: viewToModel,
		});
	});

	onUnmounted(() => {
		editor.destroy();
	});
</script>

<style>
/* Fix for modal windows with many list/checklist items */
.editorjs {
	/* Ensure the editor itself is properly sized */
	height: 100%;
	min-height: 500px; /* Minimum height to ensure visibility */
	max-height: 100%;
	overflow-y: auto;
}

/* Fix for checklist items to prevent overflow */
.cdx-checklist__item, .ce-block__content {
	word-break: break-word;
	overflow-wrap: break-word;
}

/* Make list items display properly in modal */
.cdx-list__item {
	word-wrap: break-word;
	overflow-wrap: break-word;
	padding-right: 8px;
}

/* Limit bullet point text length in modal */
.cdx-list {
	max-width: 100%;
	padding-right: 16px;
}

/* Improve readability in dark mode */
.dark .ce-block--selected .ce-block__content {
	background-color: rgba(255, 255, 255, 0.1);
}

/* Ensure content is visible within the modal */
.ce-block__content {
	max-width: calc(100% - 20px);
	margin: 0 auto;
}

/* Better checkbox appearance in checklists */
.cdx-checklist__item-checkbox {
	flex-shrink: 0;
}

/* Better text handling in checklist items */
.cdx-checklist__item-text {
	flex-grow: 1;
	word-break: break-word;
}
</style>
