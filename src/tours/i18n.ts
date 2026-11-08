import { i18n, t } from '@/i18n';
import type { StepDef, TourDef } from './types';

const camel = (segment: string): string =>
	segment.replace(/[-_]+([a-z0-9])/g, (_, char: string) => char.toUpperCase());

export const tourKey = (tourId: string): string =>
	`onboarding.tours.catalog.${tourId.split('.').map(camel).join('.')}`;

const resolve = (key: string, literal: string): string =>
	(i18n.global as any).te(key) ? (t as any)(key) : literal;

export const tourTitle = (tour: Pick<TourDef, 'id' | 'title'>): string =>
	resolve(`${tourKey(tour.id)}.title`, tour.title);

export const localizedStep = (
	tour: Pick<TourDef, 'id'>,
	step: StepDef,
): StepDef => {
	const base = `${tourKey(tour.id)}.steps.${step.id}`;
	const localized: StepDef = {
		...step,
		title: resolve(`${base}.title`, step.title),
		body: resolve(`${base}.body`, step.body),
	};
	if (step.mobileBody !== undefined)
		localized.mobileBody = resolve(`${base}.mobileBody`, step.mobileBody);
	if (step.bodyByPlatform) {
		const { macos, other } = step.bodyByPlatform;
		localized.bodyByPlatform = {
			...(macos !== undefined && {
				macos: resolve(`${base}.bodyMacos`, macos),
			}),
			...(other !== undefined && {
				other: resolve(`${base}.bodyOther`, other),
			}),
		};
	}
	return localized;
};
