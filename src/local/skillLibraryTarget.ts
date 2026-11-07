import type { ImportTarget } from '@/utils/skillImportFlow';
import { SkillLibraryError, putSkill, skillFilesSha256 } from './skillLibrary';
import { readSkillLibrary, updateSkillLibrary } from './tauriSkillLibrary';

export const localSkillTarget = (userId: number): ImportTarget => ({
	listLibrary: async () =>
		(await readSkillLibrary(userId)).skills.map((skill) => ({
			slug: skill.slug,
			version: skill.version,
			filesSha256: skillFilesSha256(skill),
		})),
	put: async (slug, markdown, fileSet, expectedFilesSha256) => {
		try {
			return await updateSkillLibrary(userId, (lib) => {
				const current = lib.skills.find((skill) => skill.slug === slug);
				if (current && skillFilesSha256(current) !== expectedFilesSha256) {
					throw Object.assign(new SkillLibraryError('files changed'), {
						response: { status: 409 },
					});
				}
				const { library, skill } = putSkill(
					lib,
					slug,
					markdown,
					new Date().toISOString(),
					fileSet,
				);
				return { library, result: { version: skill.version } };
			});
		} catch (error) {
			if (error instanceof SkillLibraryError) {
				throw Object.assign(error, {
					response: (error as any).response ?? {
						data: { message: error.message },
					},
				});
			}
			if (typeof error === 'string') {
				throw Object.assign(new Error(error), {
					response: { data: { message: error } },
				});
			}
			throw error;
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
