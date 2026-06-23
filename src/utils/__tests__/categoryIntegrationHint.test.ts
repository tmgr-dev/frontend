import {
	getCategoryIntegrationHint,
	isCategoryIntegrationConfigured,
} from '../categoryIntegrationHint';

describe('isCategoryIntegrationConfigured', () => {
	it('treats github as configured only when a repository is linked', () => {
		expect(
			isCategoryIntegrationConfigured('github', {
				hasRepository: true,
				cursorConfigured: false,
			}),
		).toBe(true);
		expect(
			isCategoryIntegrationConfigured('github', {
				hasRepository: false,
				cursorConfigured: true,
			}),
		).toBe(false);
	});

	it('treats cursor as configured only when cursor is configured', () => {
		expect(
			isCategoryIntegrationConfigured('cursor', {
				hasRepository: true,
				cursorConfigured: true,
			}),
		).toBe(true);
		expect(
			isCategoryIntegrationConfigured('cursor', {
				hasRepository: true,
				cursorConfigured: false,
			}),
		).toBe(false);
	});
});

describe('getCategoryIntegrationHint', () => {
	it('shows a hint when no GitHub repository is linked', () => {
		const hint = getCategoryIntegrationHint('github', {
			hasRepository: false,
			cursorConfigured: false,
		});
		expect(hint.show).toBe(true);
		expect(hint.kind).toBe('github');
		expect(hint.title).toMatch(/GitHub/i);
		expect(hint.ctaLabel).toBeTruthy();
	});

	it('shows a hint when no Cursor agent is configured', () => {
		const hint = getCategoryIntegrationHint('cursor', {
			hasRepository: true,
			cursorConfigured: false,
		});
		expect(hint.show).toBe(true);
		expect(hint.kind).toBe('cursor');
		expect(hint.title).toMatch(/Cursor/i);
	});

	it('does NOT show a hint when the GitHub repository is linked', () => {
		const hint = getCategoryIntegrationHint('github', {
			hasRepository: true,
			cursorConfigured: false,
		});
		expect(hint.show).toBe(false);
	});

	it('does NOT show a hint when Cursor is configured', () => {
		const hint = getCategoryIntegrationHint('cursor', {
			hasRepository: false,
			cursorConfigured: true,
		});
		expect(hint.show).toBe(false);
	});
});
