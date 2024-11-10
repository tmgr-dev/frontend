<script setup lang="ts">
	import { onMounted, ref } from 'vue';
	import { MdEditor, ExposeParam } from 'md-editor-v3';
	import 'src/assets/styles/md-editor.css';
	import store from '../store';
	import { EyeIcon, EyeSlashIcon } from '@heroicons/vue/24/outline';

	interface Props {
		modelValue: string;
		showPreview?: boolean;
	}

	const props = defineProps<Props>();
	const value = defineModel();
	const editorRef = ref<ExposeParam>();
	const editorPreviewActive = ref(false);

	const togglePreview = () => {
		editorPreviewActive.value = !editorPreviewActive.value;
		editorRef.value?.togglePreviewOnly();
	};

	onMounted(() => {
		if (props.showPreview) {
			editorPreviewActive.value = true;
			editorRef.value?.togglePreviewOnly(true);
		}
	});
</script>

<template>
	<MdEditor
		ref="editorRef"
		v-model="value"
		:preview="false"
		language="en-US"
		placeholder="Add description..."
		:theme="store.state.colorScheme === 'default' ? 'light' : 'dark'"
		:toolbars-exclude="[
			'sub',
			'sup',
			'mermaid',
			'katex',
			'revoke',
			'next',
			'save',
			'prettier',
			'fullscreen',
			'preview',
			'previewOnly',
			'htmlPreview',
			'catalog',
			'github',
		]"
		:footers="['=', 0]"
	>
		<template #defFooters>
			<button
				@click="togglePreview"
				type="button"
				class="flex h-full items-center gap-1.5 bg-gray-200/60 px-2 transition hover:bg-gray-200/90 dark:bg-gray-800/80"
			>
				<EyeSlashIcon
					v-if="editorPreviewActive"
					class="size-3.5 stroke-gray-500"
				/>
				<EyeIcon v-else class="size-3.5 stroke-gray-500" />
				<span class="text-dark-400">Preview</span>
			</button>
		</template>
	</MdEditor>
</template>

<style>
	.md-editor-dark:not(.md-editor-fullscreen) {
		--md-bk-color: transparent;
	}
</style>
