import {
	EDITOR_LABELS,
	buildDescriptionPayload,
	toUpdateBody,
	editorOptionLabel,
	normalizeEditorType,
	resolveEditorType,
} from '@/utils/editorType';

describe('normalizeEditorType', () => {
	it('accepts blockmd', () => {
		expect(normalizeEditorType('blockmd')).toBe('blockmd');
	});

	it('accepts legacy markdown and block', () => {
		expect(normalizeEditorType('markdown')).toBe('markdown');
		expect(normalizeEditorType('block')).toBe('block');
	});

	it('normalizes case and whitespace', () => {
		expect(normalizeEditorType('  BlockMD ')).toBe('blockmd');
	});

	it('returns null for unknown, empty, or non-string values', () => {
		expect(normalizeEditorType('quill')).toBeNull();
		expect(normalizeEditorType('')).toBeNull();
		expect(normalizeEditorType(null)).toBeNull();
		expect(normalizeEditorType(undefined)).toBeNull();
		expect(normalizeEditorType(42)).toBeNull();
	});

	it('has a label for every editor type', () => {
		expect(EDITOR_LABELS).toEqual({
			markdown: 'Markdown (md-editor-v3)',
			blockmd: 'Block MD (Milkdown)',
			block: 'Block (Editor.js)',
		});
	});

	it('marks the Editor.js option as legacy in the settings select', () => {
		expect(editorOptionLabel('block')).toBe('Block (Editor.js) — legacy');
		expect(editorOptionLabel('blockmd')).toBe('Block MD (Milkdown)');
		expect(editorOptionLabel('unknown')).toBe('unknown');
	});
});

const blocks = (text: string) => ({
	blocks: [{ type: 'paragraph', data: { text } }],
});
const emptyJsons = [
	null,
	undefined,
	{},
	{ blocks: [] },
	blocks('  '),
	blocks('<br>'),
];
const emptyTexts = [null, undefined, '', '  \n'];

describe('resolveEditorType', () => {
	it('opens block-only tasks in Editor.js whatever the preference', () => {
		for (const pref of ['markdown', 'blockmd', 'block'] as const) {
			for (const text of emptyTexts) {
				expect(
					resolveEditorType(
						{ description: text, description_json: blocks('hi') },
						pref,
					),
				).toBe('block');
			}
		}
	});

	it('treats a non-paragraph block as content', () => {
		const json = { blocks: [{ type: 'list', data: { items: [] } }] };
		expect(
			resolveEditorType(
				{ description: null, description_json: json },
				'markdown',
			),
		).toBe('block');
	});

	it('opens markdown-only tasks in markdown, or blockmd only when preferred', () => {
		for (const json of emptyJsons) {
			const task = { description: '# hi', description_json: json };
			expect(resolveEditorType(task, 'markdown')).toBe('markdown');
			expect(resolveEditorType(task, 'block')).toBe('markdown');
			expect(resolveEditorType(task, 'blockmd')).toBe('blockmd');
		}
	});

	it('uses the preference for empty tasks', () => {
		for (const json of emptyJsons) {
			for (const text of emptyTexts) {
				for (const pref of ['markdown', 'blockmd', 'block'] as const) {
					expect(
						resolveEditorType(
							{ description: text, description_json: json },
							pref,
						),
					).toBe(pref);
				}
			}
		}
	});

	it('lets the preference pick between two filled fields (legacy dual-write)', () => {
		const task = { description: 'md', description_json: blocks('json') };
		expect(resolveEditorType(task, 'block')).toBe('block');
		expect(resolveEditorType(task, 'markdown')).toBe('markdown');
		expect(resolveEditorType(task, 'blockmd')).toBe('blockmd');
	});

	it('does not carry the previous task editor over on a modal task switch', () => {
		const pref = 'markdown';
		const first = resolveEditorType(
			{ description: null, description_json: blocks('a') },
			pref,
		);
		const second = resolveEditorType(
			{ description: 'b', description_json: null },
			pref,
		);
		const third = resolveEditorType(
			{ description: null, description_json: null },
			pref,
		);
		expect([first, second, third]).toEqual(['block', 'markdown', 'markdown']);
	});

	it('copes with description_json delivered as a string', () => {
		expect(
			resolveEditorType(
				{ description: '', description_json: JSON.stringify(blocks('x')) },
				'markdown',
			),
		).toBe('block');
		expect(
			resolveEditorType(
				{ description: '', description_json: 'not json' },
				'markdown',
			),
		).toBe('markdown');
	});
});

describe('buildDescriptionPayload', () => {
	const task = { description: 'md', description_json: blocks('json') };

	it('block format sends the JSON and clears the markdown field', () => {
		expect(buildDescriptionPayload('block', task)).toEqual({
			description_json: task.description_json,
			clear_description: true,
		});
	});

	it.each(['markdown', 'blockmd'] as const)(
		'%s format sends the text and clears the JSON field',
		(type) => {
			expect(buildDescriptionPayload(type, task)).toEqual({
				description: 'md',
				clear_description_json: true,
			});
		},
	);

	it('sends null rather than undefined for an empty edited field', () => {
		expect(buildDescriptionPayload('block', {})).toEqual({
			description_json: null,
			clear_description: true,
		});
		expect(buildDescriptionPayload('markdown', {})).toEqual({
			description: null,
			clear_description_json: true,
		});
	});
});

describe('toUpdateBody', () => {
	it('keeps the queued format even if the editor type changed before the save went out', () => {
		const queuedAsBlock = {
			id: 1,
			description: null,
			description_json: blocks('keep'),
			...buildDescriptionPayload('block', { description_json: blocks('keep') }),
		};
		const body: Record<string, any> = toUpdateBody(queuedAsBlock);
		expect(body).not.toHaveProperty('description');
		expect(body.description_json).toEqual(blocks('keep'));
		expect(body.clear_description).toBe(true);
		expect(body).not.toHaveProperty('clear_description_json');
	});

	it('drops the JSON of a markdown snapshot', () => {
		const body = toUpdateBody({
			description: 'md',
			description_json: null,
			...buildDescriptionPayload('markdown', { description: 'md' }),
		});
		expect(body).toEqual({ description: 'md', clear_description_json: true });
	});

	it('leaves a snapshot without flags untouched', () => {
		expect(toUpdateBody({ title: 't', description: null })).toEqual({ title: 't', description: null });
	});
});
