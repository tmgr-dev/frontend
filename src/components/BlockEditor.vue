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
				console.log(event, 'Saving completed: ', outputData);
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
