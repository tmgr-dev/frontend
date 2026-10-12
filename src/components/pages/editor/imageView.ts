import { fileDisplayUrl, releaseFileDisplayUrl } from '@/actions/tmgr/files';
import { parseTmgrUrl } from '@/utils/pages/tmgrLinks';
import { imageSchema } from '@milkdown/kit/preset/commonmark';
import { $view } from '@milkdown/kit/utils';

export const createImageView = () =>
	$view(imageSchema.node, () => (node) => {
		const img = document.createElement('img');
		img.draggable = true;
		let shown: string | null = null;
		let current = '';
		const apply = (src: string, alt: string) => {
			img.alt = alt;
			if (src === current) return;
			current = src;
			const parsed = parseTmgrUrl(src);
			if (parsed?.form === 'storage' && parsed.kind === 'file') {
				fileDisplayUrl(Number(parsed.id))
					.then((url) => {
						if (current !== src) {
							releaseFileDisplayUrl(url);
							return;
						}
						releaseFileDisplayUrl(shown);
						shown = url;
						img.src = url;
					})
					.catch(() => undefined);
			} else {
				img.src = src;
			}
		};
		apply(node.attrs.src, node.attrs.alt);
		return {
			dom: img,
			update(updated) {
				if (updated.type !== node.type) return false;
				apply(updated.attrs.src, updated.attrs.alt);
				return true;
			},
			destroy() {
				current = '';
				releaseFileDisplayUrl(shown);
			},
		};
	});
