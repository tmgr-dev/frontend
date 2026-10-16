import {
	fetchFileObjectUrl,
	fileDisplayUrl,
	releaseFileDisplayUrl,
} from '@/actions/tmgr/files';
import {
	formatImageTitle,
	type ImageTitle,
	isFilenameAlt,
	parseImageTitle,
} from '@/utils/pages/imageTitle';
import { parseTmgrUrl } from '@/utils/pages/tmgrLinks';
import type { MilkdownPlugin } from '@milkdown/kit/ctx';
import { imageSchema } from '@milkdown/kit/preset/commonmark';
import {
	NodeSelection,
	Plugin,
	TextSelection,
} from '@milkdown/kit/prose/state';
import type { EditorView } from '@milkdown/kit/prose/view';
import { $prose, $view } from '@milkdown/kit/utils';
import { icons } from './icons';
import { openImageLightbox } from './imageLightbox';

export interface ImageViewOptions {
	// undefined when uploads are unavailable → hide the Replace button
	replaceImage?: (view: EditorView, pos: number, file: File) => void;
	pickImageFiles: (multiple: boolean) => Promise<File[]>;
}

type Side = 'left' | 'right' | 'nw' | 'ne' | 'sw' | 'se';

interface Resize {
	side: Side;
	x: number;
	y: number;
	start: number;
	width: number;
	ratio: number;
	min: number;
	max: number;
	mult: number;
}

const SIDES: Side[] = ['left', 'right', 'nw', 'ne', 'sw', 'se'];
const LAYOUT_ACTIONS = {
	'align-left': 'left-50',
	'align-center': null,
	'align-right': 'right-50',
	'full-width': 'full-width',
} as const;
const TOOLS = [
	['align-left', 'Align left', icons.alignLeft],
	['align-center', 'Align center', icons.alignCenter],
	['align-right', 'Align right', icons.alignRight],
	['full-width', 'Full width', icons.fullWidth],
	null,
	['download', 'Download', icons.download],
	['replace', 'Replace image', icons.replace],
	null,
	['delete', 'Delete image', icons.trash],
] as const;
const OWN_UI = '.pe-image-caption, .pe-image-toolbar, .pe-image-handle';

const downloads = new WeakMap<HTMLImageElement, () => void>();

const fileIdOf = (src: string): number | null => {
	const parsed = parseTmgrUrl(src);
	return parsed?.form === 'storage' && parsed.kind === 'file'
		? Number(parsed.id)
		: null;
};

const downloadImage = async (src: string, alt: string) => {
	const id = fileIdOf(src);
	if (id === null) {
		window.open(src, '_blank', 'noopener');
		return;
	}
	const url = await fetchFileObjectUrl(id);
	const link = document.createElement('a');
	link.href = url;
	link.download = isFilenameAlt(alt) ? alt : `image-${id}`;
	link.click();
	setTimeout(() => URL.revokeObjectURL(url), 1000);
};

const showLightbox = (view: EditorView, current: HTMLImageElement) => {
	const images = Array.from(
		view.dom.querySelectorAll<HTMLImageElement>('img.pe-image-img'),
	).filter((img) => img.naturalWidth > 0);
	openImageLightbox(
		images.map((img) => ({
			src: img.currentSrc || img.src,
			alt: img.alt,
			download: () => downloads.get(img)?.(),
		})),
		images.indexOf(current),
	);
};

const contentWidth = (element: HTMLElement) => {
	const style = getComputedStyle(element);
	return (
		element.clientWidth -
		parseFloat(style.paddingLeft) -
		parseFloat(style.paddingRight)
	);
};

const imageKeys = () =>
	$prose(
		() =>
			new Plugin({
				props: {
					handleKeyDown(view, event) {
						const { selection } = view.state;
						if (
							!(selection instanceof NodeSelection) ||
							selection.node.type.name !== 'image' ||
							event.ctrlKey ||
							event.metaKey ||
							event.altKey
						)
							return false;
						if (event.key === 'Enter') {
							const after = selection.$from.after();
							const tr = view.state.tr.insert(
								after,
								view.state.schema.nodes.paragraph.create(),
							);
							if (tr.docChanged)
								view.dispatch(
									tr
										.setSelection(TextSelection.create(tr.doc, after + 1))
										.scrollIntoView(),
								);
							return true;
						}
						if (event.key === ' ') {
							const img = (
								view.nodeDOM(selection.from) as HTMLElement | null
							)?.querySelector<HTMLImageElement>('img.pe-image-img');
							if (img?.naturalWidth) showLightbox(view, img);
							return true;
						}
						return false;
					},
				},
			}),
	);

const imageView = (options: ImageViewOptions) =>
	$view(imageSchema.node, () => (initial, view, getPos) => {
		let node = initial;
		let resize: Resize | null = null;
		let clickOpens = false;
		let shown: string | null = null;
		let current: string | null = null;

		const dom = document.createElement('span');
		dom.className = 'pe-image';
		const frame = document.createElement('span');
		frame.className = 'pe-image-frame';
		const img = document.createElement('img');
		img.className = 'pe-image-img';
		img.draggable = false;
		const failure = document.createElement('span');
		failure.className = 'pe-image-error';
		failure.innerHTML = `${icons.close}<span>Image failed to load</span>`;
		const handles = SIDES.map((side) => {
			const handle = document.createElement('span');
			handle.className = 'pe-image-handle';
			handle.dataset.side = side;
			return handle;
		});
		const toolbar = document.createElement('span');
		toolbar.className = 'pe-image-toolbar';
		for (const tool of TOOLS) {
			if (!tool) {
				toolbar.append(
					Object.assign(document.createElement('span'), {
						className: 'pe-image-toolbar-separator',
					}),
				);
				continue;
			}
			const [action, label, icon] = tool;
			if (action === 'replace' && !options.replaceImage) continue;
			const button = document.createElement('button');
			button.type = 'button';
			button.dataset.action = action;
			button.setAttribute('aria-label', label);
			button.title = label;
			button.innerHTML = icon;
			toolbar.append(button);
		}
		const caption = document.createElement('span');
		caption.className = 'pe-image-caption';
		caption.contentEditable = 'true';
		caption.dataset.placeholder = 'Write a caption';
		frame.append(img, failure, ...handles, toolbar);
		dom.append(frame, caption);

		const parts = () => parseImageTitle(node.attrs.title);
		const shownCaption = () =>
			isFilenameAlt(node.attrs.alt) ? '' : node.attrs.alt;

		const setTitle = (patch: Partial<ImageTitle>) => {
			const at = getPos();
			if (at === undefined) return;
			const before = parts();
			const tr = view.state.tr;
			if (
				(Object.keys(patch) as (keyof ImageTitle)[]).some(
					(key) => patch[key] !== before[key],
				)
			) {
				tr.setNodeMarkup(at, undefined, {
					...node.attrs,
					title: formatImageTitle({ ...before, ...patch }),
				});
			}
			view.dispatch(tr.setSelection(NodeSelection.create(tr.doc, at)));
		};

		const setFailed = (failed: boolean) =>
			dom.classList.toggle('pe-image-failed', failed);

		const load = (src: string) => {
			if (src === current) return;
			current = src;
			const id = fileIdOf(src);
			if (id === null) {
				setFailed(!src);
				if (src) img.src = src;
				return;
			}
			fileDisplayUrl(id)
				.then((url) => {
					if (current !== src) {
						releaseFileDisplayUrl(url);
						return;
					}
					releaseFileDisplayUrl(shown);
					shown = url;
					img.src = url;
				})
				.catch(() => {
					if (current === src) setFailed(true);
				});
		};

		const render = () => {
			const { layout, width, height } = parts();
			dom.dataset.layout = layout ?? 'center';
			if (!resize)
				dom.style.setProperty(
					'--image-width',
					width && layout !== 'full-width' ? `${width}px` : '',
				);
			img.style.aspectRatio =
				width && height ? `auto ${width} / ${height}` : '';
			img.alt = node.attrs.alt;
			if (document.activeElement !== caption)
				caption.textContent = shownCaption();
			for (const button of toolbar.querySelectorAll<HTMLElement>('button')) {
				const action = button.dataset.action as string;
				if (action in LAYOUT_ACTIONS)
					button.setAttribute(
						'aria-pressed',
						String(
							LAYOUT_ACTIONS[action as keyof typeof LAYOUT_ACTIONS] === layout,
						),
					);
			}
			load(node.attrs.src);
		};

		const holdDrag = () => {
			dom.draggable = false;
			document.addEventListener('pointerup', releaseDrag, { once: true });
		};
		const releaseDrag = () => {
			if (document.activeElement !== caption && !resize) dom.draggable = true;
		};

		const commitCaption = () => {
			const at = getPos();
			const text = (caption.textContent ?? '').replace(/\s+/g, ' ').trim();
			if (at === undefined || text === shownCaption()) return;
			view.dispatch(
				view.state.tr.setNodeMarkup(at, undefined, {
					...node.attrs,
					alt: text,
				}),
			);
		};

		const onMove = (event: PointerEvent) => {
			if (!resize) return;
			const { side } = resize;
			const towardLeft = side === 'left' || side === 'nw' || side === 'sw';
			let delta =
				(event.clientX - resize.x) * (towardLeft ? -1 : 1) * resize.mult;
			if (side.length === 2) {
				const byY =
					((event.clientY - resize.y) * (side[0] === 'n' ? -1 : 1)) /
					resize.ratio;
				if (Math.abs(byY) > Math.abs(delta)) delta = byY;
			}
			resize.width = Math.round(
				Math.min(resize.max, Math.max(resize.min, resize.start + delta)),
			);
			dom.style.setProperty('--image-width', `${resize.width}px`);
		};
		const onKey = (event: KeyboardEvent) => {
			if (event.key !== 'Escape') return;
			event.preventDefault();
			event.stopPropagation();
			finish(false);
		};
		const onUp = () => finish(true);
		const finish = (commit: boolean) => {
			if (!resize) return;
			const { width, start, ratio } = resize;
			resize = null;
			document.removeEventListener('pointermove', onMove);
			document.removeEventListener('pointerup', onUp);
			document.removeEventListener('pointercancel', onUp);
			document.removeEventListener('keydown', onKey, true);
			dom.classList.remove('pe-image-resizing');
			document.body.classList.remove('pe-resizing');
			document.body.style.cursor = '';
			releaseDrag();
			if (commit && width !== start) {
				setTitle({ width, height: Math.round(width * ratio) });
				view.focus();
			} else render();
		};

		const bounds = () => {
			const column = contentWidth(dom.parentElement ?? view.dom);
			const max = parts().layout ? column / 2 : column;
			const ratio = img.naturalWidth
				? img.naturalHeight / img.naturalWidth
				: img.offsetHeight / img.offsetWidth || 1;
			return { max, ratio };
		};

		for (const handle of handles) {
			handle.addEventListener('pointerdown', (event) => {
				if (event.button !== 0) return;
				event.preventDefault();
				event.stopPropagation();
				handle.setPointerCapture(event.pointerId);
				const { max, ratio } = bounds();
				resize = {
					side: handle.dataset.side as Side,
					x: event.clientX,
					y: event.clientY,
					start: img.offsetWidth,
					width: img.offsetWidth,
					ratio,
					max,
					min: Math.min(img.naturalWidth || max, max * 0.05),
					mult: parts().layout ? 1 : 2,
				};
				dom.draggable = false;
				dom.classList.add('pe-image-resizing');
				document.body.classList.add('pe-resizing');
				document.body.style.cursor = getComputedStyle(handle).cursor;
				document.addEventListener('pointermove', onMove);
				document.addEventListener('pointerup', onUp);
				document.addEventListener('pointercancel', onUp);
				document.addEventListener('keydown', onKey, true);
			});
			handle.addEventListener('dblclick', (event) => {
				event.preventDefault();
				if (!img.naturalWidth) return;
				const { max, ratio } = bounds();
				const width = Math.round(Math.min(img.naturalWidth, max));
				setTitle({ width, height: Math.round(width * ratio) });
			});
		}

		dom.addEventListener(
			'pointerdown',
			(event) => {
				if ((event.target as Element).closest(OWN_UI)) holdDrag();
			},
			true,
		);
		dom.addEventListener(
			'mousedown',
			(event) => {
				clickOpens =
					event.target === img &&
					dom.classList.contains('ProseMirror-selectednode');
			},
			true,
		);
		img.addEventListener('click', () => {
			if (clickOpens && img.naturalWidth) showLightbox(view, img);
			clickOpens = false;
		});
		img.addEventListener('load', () => setFailed(false));
		img.addEventListener('error', () => setFailed(true));
		dom.addEventListener('dragstart', () =>
			dom.classList.add('pe-image-dragging'),
		);
		dom.addEventListener('dragend', () =>
			dom.classList.remove('pe-image-dragging'),
		);

		toolbar.addEventListener('mousedown', (event) => event.preventDefault());
		toolbar.addEventListener('click', (event) => {
			const action = (event.target as Element).closest<HTMLElement>('button')
				?.dataset.action;
			if (!action) return;
			if (action in LAYOUT_ACTIONS) {
				const layout = LAYOUT_ACTIONS[action as keyof typeof LAYOUT_ACTIONS];
				setTitle({
					layout:
						action === 'full-width' && parts().layout === 'full-width'
							? null
							: layout,
				});
			} else if (action === 'download') {
				downloads.get(img)?.();
			} else if (action === 'replace') {
				void options.pickImageFiles(false).then(([file]) => {
					const at = getPos();
					if (file && at !== undefined) options.replaceImage?.(view, at, file);
				});
			} else if (action === 'delete') {
				const at = getPos();
				if (at === undefined) return;
				view.dispatch(view.state.tr.delete(at, at + node.nodeSize));
				view.focus();
			}
		});

		// A node selection's DOM range spans the caption text, and a press inside
		// a range makes Chrome drag it instead of starting a new selection.
		caption.addEventListener('pointerdown', () => {
			const selection = document.getSelection();
			if (selection && !caption.contains(selection.anchorNode))
				selection.removeAllRanges();
		});
		caption.addEventListener('blur', () => {
			releaseDrag();
			commitCaption();
		});
		caption.addEventListener('input', () => {
			if (!caption.textContent) caption.replaceChildren();
		});
		caption.addEventListener('paste', (event) => {
			event.preventDefault();
			const text = event.clipboardData?.getData('text/plain') ?? '';
			document.execCommand('insertText', false, text.replace(/\s+/g, ' '));
		});
		caption.addEventListener('drop', (event) => event.preventDefault());
		caption.addEventListener('keydown', (event) => {
			if (event.key === 'Enter' && !event.isComposing) {
				event.preventDefault();
				commitCaption();
				const at = getPos();
				if (at === undefined) return;
				const after = at + node.nodeSize;
				const tr = view.state.tr.split(after);
				view.dispatch(
					tr
						.setSelection(TextSelection.create(tr.doc, after + 2))
						.scrollIntoView(),
				);
				view.focus();
			} else if (event.key === 'Backspace' && !caption.textContent) {
				event.preventDefault();
				const at = getPos();
				if (at === undefined) return;
				view.dispatch(
					view.state.tr.setSelection(NodeSelection.create(view.state.doc, at)),
				);
				view.focus();
			}
		});

		downloads.set(img, () => {
			void downloadImage(node.attrs.src, node.attrs.alt).catch(() => undefined);
		});
		render();

		return {
			dom,
			update(updated) {
				if (updated.type !== node.type) return false;
				node = updated;
				render();
				return true;
			},
			selectNode() {
				dom.classList.add('ProseMirror-selectednode');
			},
			deselectNode() {
				dom.classList.remove('ProseMirror-selectednode');
			},
			stopEvent(event) {
				return (
					resize !== null ||
					(event.target instanceof Element && !!event.target.closest(OWN_UI))
				);
			},
			ignoreMutation: () => true,
			destroy() {
				finish(false);
				current = '';
				releaseFileDisplayUrl(shown);
			},
		};
	});

export const createImageView = (
	options: ImageViewOptions,
): MilkdownPlugin[] => [imageView(options), imageKeys()];
