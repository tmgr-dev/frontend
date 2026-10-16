import { icons } from './icons';

export interface LightboxImage {
	src: string;
	alt: string;
	download: () => void;
}

const button = (action: string, label: string, icon: string) => {
	const element = document.createElement('button');
	element.type = 'button';
	element.className = `pe-lightbox-button pe-lightbox-${action}`;
	element.dataset.action = action;
	element.setAttribute('aria-label', label);
	element.title = label;
	element.innerHTML = icon;
	return element;
};

export const openImageLightbox = (images: LightboxImage[], start: number) => {
	if (!images[start]) return;
	let index = start;
	const previous = document.activeElement as HTMLElement | null;
	const overflow = document.documentElement.style.overflow;

	const root = document.createElement('div');
	root.className = 'pe-lightbox';
	root.setAttribute('role', 'dialog');
	root.setAttribute('aria-modal', 'true');
	root.setAttribute('aria-label', 'Image viewer');
	const picture = document.createElement('img');
	picture.className = 'pe-lightbox-img';
	const prev = button('prev', 'Previous image', icons.chevronLeft);
	const next = button('next', 'Next image', icons.chevronRight);
	const download = button('download', 'Download', icons.download);
	const close = button('close', 'Close', icons.close);
	const bar = document.createElement('div');
	bar.className = 'pe-lightbox-bar';
	bar.append(download, close);
	root.append(picture, prev, next, bar);

	const show = () => {
		picture.src = images[index].src;
		picture.alt = images[index].alt;
		prev.hidden = index === 0;
		next.hidden = index === images.length - 1;
	};
	const go = (step: number) => {
		index = Math.min(images.length - 1, Math.max(0, index + step));
		show();
	};
	const shut = () => {
		window.removeEventListener('keydown', onKey, true);
		root.remove();
		document.documentElement.style.overflow = overflow;
		previous?.focus({ preventScroll: true });
	};
	const onKey = (event: KeyboardEvent) => {
		if (event.key === 'Escape') shut();
		else if (event.key === 'ArrowLeft') go(-1);
		else if (event.key === 'ArrowRight') go(1);
		else if (event.key === 'Tab') {
			const buttons = Array.from(root.querySelectorAll('button')).filter(
				(element) => !element.hidden,
			);
			const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
			const step = event.shiftKey ? -1 : 1;
			buttons[(at + step + buttons.length) % buttons.length].focus();
		} else return;
		event.preventDefault();
		event.stopPropagation();
	};

	root.addEventListener('click', (event) => {
		const action = (event.target as Element).closest<HTMLElement>('button')
			?.dataset.action;
		if (action === 'prev') go(-1);
		else if (action === 'next') go(1);
		else if (action === 'download') images[index].download();
		else if (action === 'close' || event.target === root) shut();
	});
	window.addEventListener('keydown', onKey, true);
	show();
	document.documentElement.style.overflow = 'hidden';
	document.body.append(root);
	close.focus({ preventScroll: true });
};
