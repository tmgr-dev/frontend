import { getCategories } from '@/actions/tmgr/categories';
import { getPagesTree } from '@/actions/tmgr/pages';
import { listWorkspacePersonas } from '@/actions/tmgr/personas';
import { searchTasks } from '@/actions/tmgr/taskRelations';
import { getTask } from '@/actions/tmgr/tasks';
import { getWorkspaceMembers } from '@/actions/tmgr/workspaces';
import store from '@/store';
import {
	pathForRef,
	type ParsedTmgrUrl,
	type StorageRef,
	type TmgrKind,
} from '@/utils/pages/tmgrLinks';
import { reactive } from 'vue';

export interface MentionItem {
	kind: Exclude<TmgrKind, 'file'>;
	id: string;
	title: string;
	hint?: string;
}

const key = (kind: string, id: string | number) => `${kind}/${id}`;

export const useTmgrDirectory = (getWorkspaceCode: () => string) => {
	const titles = reactive<Record<string, string>>({});
	const pageSlugs = reactive<Record<string, string>>({});
	const categoryCodes = reactive<Record<string, string>>({});
	const workspaceId = () => store.getters.currentWorkspaceId as number | null;

	const loadPages = async () => {
		const tree = await getPagesTree();
		for (const page of tree) {
			titles[key('page', page.id)] = page.title;
			pageSlugs[String(page.id)] = page.slug;
		}
		return tree;
	};

	const loadCategories = async () => {
		const categories = await getCategories();
		for (const category of categories) {
			titles[key('category', category.id)] = category.title;
			if (category.code) categoryCodes[String(category.id)] = category.code;
		}
		return categories;
	};

	const loadMembers = async () => {
		const id = workspaceId();
		if (!id) return [];
		const members = await getWorkspaceMembers(id);
		for (const member of members) {
			titles[key('user', member.id)] = member.name;
		}
		return members;
	};

	const loadPersonas = async () => {
		const id = workspaceId();
		if (!id || !store.getters['featureToggles/isFeatureEnabled']('personas'))
			return [];
		const grants = await listWorkspacePersonas(id);
		for (const grant of grants) {
			titles[key('persona', grant.persona.id)] = grant.persona.name;
		}
		return grants;
	};

	const ensure = async (refs: StorageRef[]): Promise<void> => {
		const kinds = new Set(refs.map((ref) => ref.kind));
		const jobs: Promise<unknown>[] = [];
		if (kinds.has('page')) jobs.push(loadPages());
		if (kinds.has('category')) jobs.push(loadCategories());
		if (kinds.has('user')) jobs.push(loadMembers());
		if (kinds.has('persona')) jobs.push(loadPersonas());
		for (const ref of refs) {
			if (ref.kind !== 'task' || key('task', ref.id) in titles) continue;
			jobs.push(
				getTask(Number(ref.id), true).then((task) => {
					titles[key('task', ref.id)] = task.title;
				}),
			);
		}
		await Promise.allSettled(jobs);
	};

	const titleFor = (kind: string, id: string | number): string | null =>
		titles[key(kind, id)] ?? null;

	const pathFor = (ref: StorageRef | ParsedTmgrUrl): string | null =>
		pathForRef(ref, {
			workspaceCode: getWorkspaceCode(),
			pageSlugs,
			categoryCodes,
		});

	const includes = (value: string, query: string) =>
		value.toLowerCase().includes(query.toLowerCase());

	const searchMentions = async (
		query: string,
		pagesOnly: boolean,
	): Promise<MentionItem[]> => {
		const pages = loadPages()
			.then((tree) =>
				tree
					.filter((page) => includes(page.title, query))
					.slice(0, 8)
					.map(
						(page): MentionItem => ({
							kind: 'page',
							id: String(page.id),
							title: page.title,
							hint: page.type === 'plain' ? undefined : page.type,
						}),
					),
			)
			.catch(() => [] as MentionItem[]);
		if (pagesOnly) return pages;

		const tasks =
			query.length >= 2
				? searchTasks(query, 6)
						.then((found) =>
							found.map(
								(task): MentionItem => ({
									kind: 'task',
									id: String(task.id),
									title: task.title,
								}),
							),
						)
						.catch(() => [] as MentionItem[])
				: Promise.resolve([] as MentionItem[]);
		const categories = loadCategories()
			.then((list) =>
				list
					.filter((category) => includes(category.title, query))
					.slice(0, 4)
					.map(
						(category): MentionItem => ({
							kind: 'category',
							id: String(category.id),
							title: category.title,
						}),
					),
			)
			.catch(() => [] as MentionItem[]);
		const members = loadMembers()
			.then((list) =>
				list
					.filter((member) => includes(member.name, query))
					.slice(0, 4)
					.map(
						(member): MentionItem => ({
							kind: 'user',
							id: String(member.id),
							title: member.name,
						}),
					),
			)
			.catch(() => [] as MentionItem[]);
		const personas = loadPersonas()
			.then((list) =>
				list
					.filter(
						(grant) => !grant.blocked && includes(grant.persona.name, query),
					)
					.slice(0, 4)
					.map(
						(grant): MentionItem => ({
							kind: 'persona',
							id: grant.persona.id,
							title: grant.persona.name,
						}),
					),
			)
			.catch(() => [] as MentionItem[]);

		const groups = await Promise.all([
			pages,
			tasks,
			categories,
			members,
			personas,
		]);
		return groups.flat();
	};

	const searchPeople = async (query: string): Promise<MentionItem[]> => {
		const pages = loadPages()
			.then((tree) =>
				tree
					.filter(
						(page) => page.type === 'person' && includes(page.title, query),
					)
					.slice(0, 8)
					.map(
						(page): MentionItem => ({
							kind: 'page',
							id: String(page.id),
							title: page.title,
						}),
					),
			)
			.catch(() => [] as MentionItem[]);
		const members = searchMembers(query);
		return (await Promise.all([pages, members])).flat();
	};

	const searchMembers = (query: string): Promise<MentionItem[]> =>
		loadMembers()
			.then((list) =>
				list
					.filter((member) => includes(member.name, query))
					.slice(0, 8)
					.map(
						(member): MentionItem => ({
							kind: 'user',
							id: String(member.id),
							title: member.name,
						}),
					),
			)
			.catch(() => [] as MentionItem[]);

	const searchTaskItems = (query: string): Promise<MentionItem[]> =>
		query.length < 2
			? Promise.resolve([])
			: searchTasks(query, 8)
					.then((found) =>
						found.map(
							(task): MentionItem => ({
								kind: 'task',
								id: String(task.id),
								title: task.title,
							}),
						),
					)
					.catch(() => [] as MentionItem[]);

	return {
		titles,
		ensure,
		titleFor,
		pathFor,
		searchMentions,
		searchPeople,
		searchMembers,
		searchTaskItems,
	};
};

export type TmgrDirectory = ReturnType<typeof useTmgrDirectory>;
