import { i18n, t } from '@/i18n';
import { TOURS } from '../catalog';
import { localizedStep, tourKey, tourTitle } from '../i18n';

const translated = (key: string, literal: string | undefined) => {
	if (literal === undefined) return;
	expect(`${key}: ${(i18n.global as any).te(key)}`).toBe(`${key}: true`);
	expect(t(key)).toBe(literal);
};

describe('tour catalog messages', () => {
	it('every tour and step text has an English message equal to the catalog literal', () => {
		for (const tour of TOURS) {
			const base = tourKey(tour.id);
			translated(`${base}.title`, tour.title);
			expect(tourTitle(tour)).toBe(tour.title);
			for (const step of tour.steps) {
				const key = `${base}.steps.${step.id}`;
				translated(`${key}.title`, step.title);
				translated(`${key}.body`, step.body);
				translated(`${key}.mobileBody`, step.mobileBody);
				translated(`${key}.bodyMacos`, step.bodyByPlatform?.macos);
				translated(`${key}.bodyOther`, step.bodyByPlatform?.other);
				expect(localizedStep(tour, step)).toEqual(step);
			}
		}
	});

	it('falls back to the literal text for tours without messages', () => {
		const step = { id: 's1', type: 'tip' as const, title: 'T', body: 'B' };
		expect(localizedStep({ id: 'unknown.tour' }, step)).toEqual(step);
		expect(tourTitle({ id: 'unknown.tour', title: 'Own' })).toBe('Own');
	});
});
