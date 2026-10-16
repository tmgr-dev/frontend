import { parseTmgrUrl } from '@/utils/pages/tmgrLinks';
import { syntaxHighlighting } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { shift, size } from '@floating-ui/dom';
import { classHighlighter } from '@lezer/highlight';
import { Crepe } from '@milkdown/crepe';
import {
	commandsCtx,
	editorViewCtx,
	editorViewOptionsCtx,
	remarkStringifyOptionsCtx,
} from '@milkdown/kit/core';
import type { Ctx } from '@milkdown/kit/ctx';
import { uploadConfig } from '@milkdown/kit/plugin/upload';
import {
	clearTextInCurrentBlockCommand,
	linkAttr,
} from '@milkdown/kit/preset/commonmark';
import { remarkGFMPlugin } from '@milkdown/kit/preset/gfm';
import type { TmgrDirectory } from '../useTmgrDirectory';
import { icons } from './icons';
import { createImageView } from './imageView';
import { createImageUpload, pickImageFiles } from './upload';

export interface PageCrepeOptions {
	root: HTMLElement;
	markdown: string;
	placeholder: string;
	directory: TmgrDirectory;
	uploadFile?: (file: File) => Promise<number>;
	onUploadError: (file: File) => void;
	onMakeTask: (text: string) => void;
}

const item = (label: string, icon: string) => ({ label, icon });

export const createPageCrepe = (options: PageCrepeOptions): Crepe => {
	const upload = createImageUpload(options);

	const pickImages = async (ctx: Ctx) => {
		ctx.get(commandsCtx).call(clearTextInCurrentBlockCommand.key);
		const view = ctx.get(editorViewCtx);
		const at = view.state.selection.from;
		const files = await pickImageFiles(true);
		if (!files.length || view.isDestroyed) return;
		void upload.insertImages(
			view,
			Math.min(at, view.state.doc.content.size),
			files,
		);
	};

	const crepe = new Crepe({
		root: options.root,
		defaultValue: options.markdown,
		features: {
			[Crepe.Feature.Latex]: false,
			[Crepe.Feature.ImageBlock]: false,
		},
		featureConfigs: {
			[Crepe.Feature.BlockEdit]: {
				handleAddIcon: icons.plus,
				handleDragIcon: icons.grip,
				blockHandle: { getOffset: () => 6 },
				buildMenu: (builder) => {
					if (!options.uploadFile) return;
					builder.getGroup('advanced').addItem('image', {
						label: 'Image',
						icon: icons.image,
						onRun: pickImages,
					});
				},
				slashMenu: {
					middleware: [
						shift({ padding: 8 }),
						size({
							padding: 8,
							apply: ({ availableHeight, elements }) => {
								elements.floating.style.setProperty(
									'--pe-menu-max',
									`${Math.max(160, availableHeight - 8)}px`,
								);
							},
						}),
					],
				},
				textGroup: {
					label: 'Text',
					text: item('Text', icons.text),
					h1: item('Heading 1', icons.h1),
					h2: item('Heading 2', icons.h2),
					h3: item('Heading 3', icons.h3),
					h4: null,
					h5: null,
					h6: null,
					quote: item('Quote', icons.quote),
					divider: item('Divider', icons.divider),
				},
				listGroup: {
					label: 'Lists',
					bulletList: item('Bulleted list', icons.bulletList),
					orderedList: item('Numbered list', icons.orderedList),
					taskList: item('Checklist', icons.taskList),
				},
				advancedGroup: {
					label: 'Insert',
					codeBlock: item('Code block', icons.code),
					table: item('Table', icons.table),
				},
			},
			[Crepe.Feature.Toolbar]: {
				boldIcon: icons.bold,
				italicIcon: icons.italic,
				strikethroughIcon: icons.strike,
				codeIcon: icons.code,
				linkIcon: icons.link,
				buildToolbar: (builder) => {
					builder.addGroup('tmgr', 'Page').addItem('make-task', {
						label: 'Make a task',
						icon: icons.makeTask,
						active: () => false,
						onRun: (ctx) => {
							const { state } = ctx.get(editorViewCtx);
							const { from, to } = state.selection;
							const text = state.doc.textBetween(from, to, '\n').trim();
							if (text) options.onMakeTask(text);
						},
					});
				},
			},
			[Crepe.Feature.ListItem]: {
				bulletIcon: icons.bullet,
				checkBoxCheckedIcon: icons.checked,
				checkBoxUncheckedIcon: icons.unchecked,
			},
			[Crepe.Feature.CodeMirror]: {
				languages,
				theme: syntaxHighlighting(classHighlighter),
				searchPlaceholder: 'Search language',
				noResultText: 'No language found',
			},
			[Crepe.Feature.Placeholder]: {
				text: options.placeholder,
				mode: 'block',
			},
		},
	});

	crepe.editor.use(upload.plugin).use(
		createImageView({
			replaceImage: options.uploadFile ? upload.replaceImage : undefined,
			pickImageFiles,
		}),
	);
	crepe.editor.config((ctx) => {
		ctx.update(remarkStringifyOptionsCtx, (prev) => ({
			...prev,
			bullet: '-' as const,
			rule: '-' as const,
		}));
		ctx.set(remarkGFMPlugin.options.key, { stringLength: () => 3 });
		ctx.update(uploadConfig.key, (prev) => ({
			...prev,
			uploader: async () => [],
		}));
		ctx.update(editorViewOptionsCtx, (prev) => ({
			...prev,
			handlePaste: upload.handlePaste,
			handleDrop: upload.handleDrop,
		}));
		ctx.set(linkAttr.key, (mark) => {
			const href = String(mark.attrs.href ?? '');
			const parsed = parseTmgrUrl(href);
			if (!parsed || parsed.form !== 'storage') {
				return parsed ? { 'data-tmgr': href, class: 'tmgr-chip' } : {};
			}
			const title = options.directory.titleFor(parsed.kind, parsed.id);
			return {
				'data-tmgr': href,
				class: `tmgr-chip tmgr-chip-${parsed.kind}`,
				...(title ? { 'data-current-title': title, 'aria-label': title } : {}),
			};
		});
	});

	return crepe;
};
