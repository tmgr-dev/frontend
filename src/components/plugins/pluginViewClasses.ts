import type { Gap } from '@/pluginSystem/uiTree';

export type StackAlign = 'start' | 'center' | 'end' | 'stretch';
export type StackJustify = 'start' | 'between' | 'end';
export type CardTone = 'default' | 'muted' | 'raised';
export type CardPadding = 'sm' | 'md' | 'lg';
export type GridGap = 'sm' | 'md' | 'lg';
export type ButtonVariant = 'default' | 'primary' | 'ghost';
export type ButtonSize = 'sm' | 'md';

export const STACK_GAP: Record<Gap, string> = {
	none: 'gap-0',
	sm: 'gap-1.5',
	md: 'gap-3',
	lg: 'gap-5',
};

export const STACK_ALIGN: Record<StackAlign, string> = {
	start: 'items-start',
	center: 'items-center',
	end: 'items-end',
	stretch: 'items-stretch',
};

export const STACK_JUSTIFY: Record<StackJustify, string> = {
	start: 'justify-start',
	between: 'justify-between',
	end: 'justify-end',
};

export const CARD_TONE: Record<CardTone, string> = {
	default: 'bg-card text-card-foreground',
	muted: 'bg-muted/50',
	raised: 'bg-card shadow-sm',
};

export const CARD_PADDING: Record<CardPadding, string> = {
	sm: 'p-2',
	md: 'p-3',
	lg: 'p-4',
};

export const GRID_GAP: Record<GridGap, string> = {
	sm: 'gap-2',
	md: 'gap-3',
	lg: 'gap-5',
};

export const BUTTON_VARIANT: Record<ButtonVariant, 'outline' | 'default' | 'ghost'> = {
	default: 'outline',
	primary: 'default',
	ghost: 'ghost',
};

export const BUTTON_SIZE: Record<ButtonSize, 'sm' | 'default'> = {
	sm: 'sm',
	md: 'default',
};

export const stackClasses = (node: {
	direction: 'row' | 'column';
	gap?: Gap;
	align?: StackAlign;
	justify?: StackJustify;
	grow?: boolean;
}): string[] => {
	const classes = ['flex', node.gap ? STACK_GAP[node.gap] : 'gap-3'];
	classes.push(node.direction === 'row' ? 'flex-row flex-wrap' : 'flex-col');
	const align = node.align
		? STACK_ALIGN[node.align]
		: node.direction === 'row'
		? 'items-stretch'
		: '';
	if (align) classes.push(align);
	if (node.justify) classes.push(STACK_JUSTIFY[node.justify]);
	if (node.grow) classes.push('min-w-0', 'flex-1');
	return classes;
};

export const cardOuterClasses = (node: {
	tone: CardTone;
	onClick?: unknown;
	grow?: boolean;
}): string[] => {
	const classes = [
		'rounded-xl',
		'border',
		'border-border',
		'overflow-hidden',
		CARD_TONE[node.tone],
	];
	if (node.grow) classes.push('min-w-0', 'flex-1');
	if (node.onClick) {
		classes.push(
			'text-left',
			'cursor-pointer',
			'transition-colors',
			'hover:bg-accent',
			'hover:text-accent-foreground',
			'focus-visible:outline-none',
			'focus-visible:ring-2',
			'focus-visible:ring-ring',
			'focus-visible:ring-offset-2',
		);
	}
	return classes;
};

export const cardInnerClasses = (node: { padding: CardPadding }): string[] => [
	'flex',
	'flex-col',
	'gap-3',
	CARD_PADDING[node.padding],
];

export const gridInnerClasses = (node: { gap: GridGap }): string[] => [
	'grid',
	GRID_GAP[node.gap],
];

export const gridTemplateStyle = (node: {
	columns: number;
	minWidth: number;
}): { gridTemplateColumns: string } => ({
	gridTemplateColumns: `repeat(${node.columns}, minmax(${node.minWidth}px, 1fr))`,
});

export const buttonVariantProp = (variant: ButtonVariant | undefined) =>
	BUTTON_VARIANT[variant ?? 'default'];

export const buttonSizeProp = (size: ButtonSize | undefined) =>
	BUTTON_SIZE[size ?? 'sm'];

export const cardKeyActivates = (key: string, isSelf: boolean): boolean =>
	isSelf && (key === 'Enter' || key === ' ');

/** Duck-typed so this can be exercised without a DOM (jest runs node env, no jsdom). Real DOM
 * `Element`s satisfy this shape already. */
export interface InteractiveLike {
	tagName?: string;
	getAttribute?(name: string): string | null;
	parentElement: InteractiveLike | null;
}

const isInteractiveNode = (el: InteractiveLike): boolean => {
	const tag = el.tagName?.toLowerCase();
	if (tag === 'button' || tag === 'a' || tag === 'input') return true;
	const role = el.getAttribute?.('role');
	return role === 'menuitem' || role === 'button';
};

export const isNestedInteractive = (
	target: InteractiveLike | null,
	card: InteractiveLike | null,
): boolean => {
	let el = target;
	while (el && el !== card) {
		if (isInteractiveNode(el)) return true;
		el = el.parentElement;
	}
	return false;
};
