import {
	BUTTON_SIZE,
	BUTTON_VARIANT,
	CARD_PADDING,
	CARD_TONE,
	GRID_GAP,
	STACK_ALIGN,
	STACK_GAP,
	STACK_JUSTIFY,
	buttonSizeProp,
	buttonVariantProp,
	cardInnerClasses,
	cardKeyActivates,
	cardOuterClasses,
	gridInnerClasses,
	gridTemplateStyle,
	isNestedInteractive,
	statClasses,
	type InteractiveLike,
} from '../pluginViewClasses';

describe('class maps', () => {
	it('STACK_GAP', () => {
		expect(STACK_GAP).toMatchInlineSnapshot(`
			{
			  "lg": "gap-5",
			  "md": "gap-3",
			  "none": "gap-0",
			  "sm": "gap-1.5",
			}
		`);
	});

	it('STACK_ALIGN', () => {
		expect(STACK_ALIGN).toMatchInlineSnapshot(`
			{
			  "center": "items-center",
			  "end": "items-end",
			  "start": "items-start",
			  "stretch": "items-stretch",
			}
		`);
	});

	it('STACK_JUSTIFY', () => {
		expect(STACK_JUSTIFY).toMatchInlineSnapshot(`
			{
			  "between": "justify-between",
			  "end": "justify-end",
			  "start": "justify-start",
			}
		`);
	});

	it('CARD_TONE', () => {
		expect(CARD_TONE).toMatchInlineSnapshot(`
			{
			  "default": "bg-card text-card-foreground",
			  "muted": "bg-muted/50",
			  "raised": "bg-card shadow-sm",
			}
		`);
	});

	it('CARD_PADDING', () => {
		expect(CARD_PADDING).toMatchInlineSnapshot(`
			{
			  "lg": "p-4",
			  "md": "p-3",
			  "sm": "p-2",
			}
		`);
	});

	it('GRID_GAP', () => {
		expect(GRID_GAP).toMatchInlineSnapshot(`
			{
			  "lg": "gap-5",
			  "md": "gap-3",
			  "sm": "gap-2",
			}
		`);
	});

	it('BUTTON_VARIANT maps to shadcn Button variant names', () => {
		expect(BUTTON_VARIANT).toMatchInlineSnapshot(`
			{
			  "default": "outline",
			  "ghost": "ghost",
			  "primary": "default",
			}
		`);
	});

	it('BUTTON_SIZE maps to shadcn Button size names', () => {
		expect(BUTTON_SIZE).toMatchInlineSnapshot(`
			{
			  "md": "default",
			  "sm": "sm",
			}
		`);
	});

	it('buttonVariantProp/buttonSizeProp default to today\'s outline/sm when absent', () => {
		expect(buttonVariantProp(undefined)).toBe('outline');
		expect(buttonSizeProp(undefined)).toBe('sm');
	});

	const COLOR_CLASS = /\b(?:bg|text|border)-(?:red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-\d+/;

	it('every hardcoded color class in the maps has a dark: pair, or is a theme token', () => {
		const allClasses = [
			...Object.values(STACK_GAP),
			...Object.values(STACK_ALIGN),
			...Object.values(STACK_JUSTIFY),
			...Object.values(CARD_TONE),
			...Object.values(CARD_PADDING),
			...Object.values(GRID_GAP),
		];
		for (const classString of allClasses) {
			const hardcoded = classString.match(COLOR_CLASS);
			if (!hardcoded) continue;
			expect(classString).toMatch(/dark:/);
		}
	});
});

describe('stack/card/grid class builders', () => {
	it('cardOuterClasses adds interactive affordances only when onClick is set', () => {
		const plain = cardOuterClasses({ tone: 'default' });
		expect(plain).not.toContain('cursor-pointer');
		const clickable = cardOuterClasses({
			tone: 'default',
			onClick: { command: 'x' },
		});
		expect(clickable).toContain('cursor-pointer');
		expect(clickable).toContain('focus-visible:ring-2');
	});

	it('cardOuterClasses adds grow classes', () => {
		expect(cardOuterClasses({ tone: 'muted', grow: true })).toEqual(
			expect.arrayContaining(['min-w-0', 'flex-1']),
		);
	});

	it('cardInnerClasses applies padding', () => {
		expect(cardInnerClasses({ padding: 'lg' })).toEqual(
			expect.arrayContaining(['p-4']),
		);
	});

	it('gridInnerClasses applies gap', () => {
		expect(gridInnerClasses({ gap: 'sm' })).toEqual(
			expect.arrayContaining(['gap-2']),
		);
	});

	it('gridTemplateStyle builds an inline gridTemplateColumns', () => {
		expect(gridTemplateStyle({ columns: 4, minWidth: 180 })).toEqual({
			gridTemplateColumns: 'repeat(4, minmax(180px, 1fr))',
		});
	});
});

describe('cardKeyActivates', () => {
	it('activates on Enter or Space only when the event target is the card itself', () => {
		expect(cardKeyActivates('Enter', true)).toBe(true);
		expect(cardKeyActivates(' ', true)).toBe(true);
		expect(cardKeyActivates('Enter', false)).toBe(false);
		expect(cardKeyActivates(' ', false)).toBe(false);
		expect(cardKeyActivates('Tab', true)).toBe(false);
	});
});

describe('isNestedInteractive', () => {
	const el = (
		tagName: string,
		attrs: Record<string, string> = {},
		parentElement: InteractiveLike | null = null,
	): InteractiveLike => ({
		tagName,
		getAttribute: (name) => attrs[name] ?? null,
		parentElement,
	});

	it('is false when the target is the card itself', () => {
		const card = el('div', { role: 'button' });
		expect(isNestedInteractive(card, card)).toBe(false);
	});

	it('is false for plain nested content', () => {
		const card = el('div', { role: 'button' });
		const span = el('span', {}, card);
		expect(isNestedInteractive(span, card)).toBe(false);
	});

	it('is true for a nested button, link, input or [role=menuitem]', () => {
		const card = el('div', { role: 'button' });
		for (const node of [
			el('button', {}, card),
			el('a', {}, card),
			el('input', {}, card),
			el('div', { role: 'menuitem' }, card),
		]) {
			expect(isNestedInteractive(node, card)).toBe(true);
		}
	});

	it('is true for a nested [role=button] other than the card itself', () => {
		const card = el('div', { role: 'button' });
		const nestedCard = el('div', { role: 'button' }, card);
		const textInsideNestedCard = el('span', {}, nestedCard);
		expect(isNestedInteractive(nestedCard, card)).toBe(true);
		expect(isNestedInteractive(textInsideNestedCard, card)).toBe(true);
	});

	it('is false when target/card are missing', () => {
		expect(isNestedInteractive(null, null)).toBe(false);
	});
});

describe('statClasses', () => {
	it('lays a clickable stat out exactly like a plain one, with an automatic height', () => {
		const plain = statClasses(false);
		const clickable = statClasses(true);
		expect(clickable).toEqual(expect.arrayContaining(plain));
		expect(plain).toContain('h-auto');
		expect(clickable).toContain('h-auto');
		expect(clickable.filter((name) => /^h-\d/.test(name))).toEqual([]);
	});

	it('only adds interaction styles when clickable', () => {
		expect(statClasses(false)).not.toContain('cursor-pointer');
		expect(statClasses(true)).toContain('cursor-pointer');
		expect(statClasses(true)).toContain('hover:bg-accent');
	});
});
