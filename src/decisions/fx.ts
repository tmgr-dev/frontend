export const prefersReducedMotion = (): boolean => {
	try {
		return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
	} catch {
		return true;
	}
};

const PARTICLES = 16;
const COLORS = ['#22c55e', '#f59e0b', '#ef4444', '#38bdf8', '#a855f7'];

const dispose = (node: HTMLElement, ms: number) => {
	window.setTimeout(() => node.remove(), ms);
};

export const flyOut = (source: Element | null): void => {
	if (!source || prefersReducedMotion()) return;
	const rect = source.getBoundingClientRect();
	if (rect.width === 0 || rect.height === 0) return;
	const ghost = source.cloneNode(true) as HTMLElement;
	ghost.removeAttribute('id');
	ghost.setAttribute('aria-hidden', 'true');
	ghost.inert = true;
	ghost.classList.add('doom-ghost');
	Object.assign(ghost.style, {
		top: `${rect.top}px`,
		left: `${rect.left}px`,
		width: `${rect.width}px`,
		height: `${rect.height}px`,
	});
	document.body.appendChild(ghost);
	dispose(ghost, 520);
};

export const burst = (random: () => number = Math.random): void => {
	if (prefersReducedMotion()) return;
	const host = document.createElement('div');
	host.className = 'doom-burst';
	host.setAttribute('aria-hidden', 'true');
	for (let i = 0; i < PARTICLES; i += 1) {
		const angle = (Math.PI * 2 * i) / PARTICLES + random() * 0.4;
		const distance = 70 + random() * 90;
		const bit = document.createElement('i');
		bit.style.setProperty('--dx', `${Math.cos(angle) * distance}px`);
		bit.style.setProperty('--dy', `${Math.sin(angle) * distance}px`);
		bit.style.background = COLORS[i % COLORS.length];
		host.appendChild(bit);
	}
	document.body.appendChild(host);
	dispose(host, 1000);
};
