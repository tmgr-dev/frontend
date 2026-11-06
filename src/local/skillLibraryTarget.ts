import type { ImportTarget } from '@/utils/skillImportFlow';
import { SkillLibraryError, putSkill } from './skillLibrary';
import { readSkillLibrary, updateSkillLibrary } from './tauriSkillLibrary';

export const localSkillTarget = (userId: number): ImportTarget => ({
	listLibrary: async () =>
		(await readSkillLibrary(userId)).skills.map(({ slug, version }) => ({
			slug,
			version,
		})),
	put: async (slug, markdown) => {
		try {
			return await updateSkillLibrary(userId, (lib) => {
				const { library, skill } = putSkill(
					lib,
					slug,
					markdown,
					new Date().toISOString(),
				);
				return { library, result: { version: skill.version } };
			});
		} catch (error) {
			if (!(error instanceof SkillLibraryError)) throw error;
			throw Object.assign(error, {
				response: { data: { message: error.message } },
			});
		}
	},
	getBody: async (slug) =>
		(await readSkillLibrary(userId)).skills.find((s) => s.slug === slug)
			?.markdown ?? '',
	loadIndex: async () => (await readSkillLibrary(userId)).links,
	saveIndex: async (index) => {
		try {
			await updateSkillLibrary(userId, (lib) => ({
				library: { ...lib, links: index },
				result: true,
			}));
			return true;
		} catch {
			return false;
		}
	},
});
