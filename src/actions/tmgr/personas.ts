import $axios from '@/plugins/axios';
import type { PersonaAssignee, PersonaPolicy } from '@/utils/personas';
import { requestCache } from '@/utils/requestCache';

export interface PersonaOwnerRef {
	id: number;
	name: string;
}

export type PersonaOwnerNotifications = 'all' | 'mentions' | 'none';

export interface Persona {
	id: string;
	name: string;
	description: string | null;
	avatar_url: string | null;
	system_prompt?: string;
	owner_notifications?: PersonaOwnerNotifications;
	prompt_version?: number;
	archived_at: string | null;
	created_at: string;
	updated_at: string;
	owner: PersonaOwnerRef;
	workspace_id?: number | null;
	scope?: 'account' | 'workspace';
	can_edit?: boolean;
}

export interface PersonaSkillTitle {
	slug: string;
	title: string;
}

export interface PersonaGrantSummary {
	id: string;
	name: string;
	description: string | null;
	avatar_url: string | null;
	archived: boolean;
	owner: PersonaOwnerRef;
	workspace_id?: number | null;
	skills?: PersonaSkillTitle[];
}

export interface PersonaSkillSummary {
	slug: string;
	title: string;
	when: string;
	actions: string[];
	version: number;
	updated_at: string;
}

export interface PersonaSkill extends PersonaSkillSummary {
	body: string;
}

export interface SharedSkillPersonaRef {
	id: string;
	name: string;
}

export interface SharedSkillAttachment extends SharedSkillPersonaRef {
	overridden: boolean;
	pinned_version: number | null;
}

export interface SharedSkillSummary extends PersonaSkillSummary {
	attached_personas: SharedSkillAttachment[];
	files_count?: number;
	files_sha256?: string;
}

export interface SharedSkill extends SharedSkillSummary {
	body: string;
}

export interface PersonaSharedSkill extends PersonaSkillSummary {
	overridden: boolean;
	pinned_version: number | null;
	latest_version: number;
	files_count?: number;
}

export type SharedSkillVersionKind =
	'baseline' | 'create' | 'edit' | 'restore' | 'convert';

export interface SharedSkillVersionAuthor {
	id: number;
	name: string;
}

export interface SharedSkillVersionRow {
	version: number;
	kind: SharedSkillVersionKind;
	restored_from: number | null;
	title: string;
	author: SharedSkillVersionAuthor | null;
	created_at: string;
	current: boolean;
	pinned_personas?: SharedSkillPersonaRef[];
}

export interface SharedSkillVersion extends SharedSkillVersionRow {
	when: string;
	actions: unknown[];
	body: string;
}

export interface DeletedSharedSkillVersionRow {
	version: number;
	title: string;
	kind: SharedSkillVersionKind;
	restored_from: number | null;
	author_name: string | null;
	created_at: string;
}

export interface DeletedSharedSkill {
	slug: string;
	title: string;
	deleted_at: string;
	purge_at: string;
	latest_version: number;
	files_count?: number;
	versions: DeletedSharedSkillVersionRow[];
}

export interface DeletedSharedSkillVersion extends DeletedSharedSkillVersionRow {
	when: string;
	actions: unknown[];
	body: string;
}

export interface PersonaGrant {
	persona: PersonaGrantSummary;
	workspace_id: number;
	permissions: string[];
	blocked: boolean;
	blocked_at: string | null;
	effective_permissions: string[];
}

export interface PersonaPolicyResponse {
	policy: PersonaPolicy;
	updated_at: string;
}

export interface PersonaInput {
	name: string;
	description?: string;
	system_prompt?: string;
	owner_notifications?: PersonaOwnerNotifications;
	workspace_id?: number;
}

export interface PersonaToken {
	id: number;
	persona_id: string;
	workspace_id: number;
	workspace_name: string;
	prefix: string;
	label: string;
	expires_at: string;
	last_used_at: string | null;
	revoked_at: string | null;
	created_at: string;
}

export interface IssuePersonaTokenPayload {
	workspace_id: number;
	label: string;
	expires_in_days: number;
}

export interface IssuedPersonaToken {
	token: PersonaToken;
	secret: string;
}

const PERSONAS_KEY = 'personas';
const invalidatePersonas = () => requestCache.invalidate(/^personas/);
const workspaceGrantsKey = (workspaceId: number) =>
	`ws-${workspaceId}-persona-grants`;
const workspacePolicyKey = (workspaceId: number) =>
	`ws-${workspaceId}-persona-policy`;
const assignableKey = (workspaceId: number) =>
	`personas-assignable-ws-${workspaceId}`;
const invalidateAssignable = (workspaceId: number) =>
	requestCache.invalidate(assignableKey(workspaceId));

export const listPersonas = async (
	includeArchived = false,
	useCache = true,
): Promise<Persona[]> =>
	requestCache.getOrFetch(
		includeArchived ? `${PERSONAS_KEY}-archived` : PERSONAS_KEY,
		async () => {
			const {
				data: { data },
			} = await $axios.get('/personas', {
				params: includeArchived ? { archived: 1 } : undefined,
			});
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);

export const getPersona = async (uuid: string): Promise<Persona> => {
	const {
		data: { data },
	} = await $axios.get(`/personas/${uuid}`);
	return data;
};

export const createPersona = async (
	payload: PersonaInput,
): Promise<Persona> => {
	const {
		data: { data },
	} = await $axios.post('/personas', payload);
	invalidatePersonas();
	return data;
};

export const updatePersona = async (
	uuid: string,
	payload: Partial<PersonaInput>,
): Promise<Persona> => {
	const {
		data: { data },
	} = await $axios.put(`/personas/${uuid}`, payload);
	invalidatePersonas();
	return data;
};

export const archivePersona = async (uuid: string): Promise<void> => {
	await $axios.delete(`/personas/${uuid}`);
	invalidatePersonas();
};

export const restorePersona = async (uuid: string): Promise<Persona> => {
	const {
		data: { data },
	} = await $axios.post(`/personas/${uuid}/restore`);
	invalidatePersonas();
	return data;
};

export const uploadPersonaAvatar = async (
	uuid: string,
	file: File,
): Promise<Persona> => {
	const form = new FormData();
	form.append('avatar', file);
	const {
		data: { data },
	} = await $axios.post(`/personas/${uuid}/avatar`, form, {
		headers: { 'Content-Type': 'multipart/form-data' },
	});
	invalidatePersonas();
	forgetPersonaAvatar(uuid);
	return data;
};

const avatarObjectUrls = new Map<string, string>();

/** Persona avatars are auth-protected with no signed link (yet) — fetched as a blob, not a plain <img src>. */
export const personaAvatarObjectUrl = async (
	uuid: string,
): Promise<string | null> => {
	const cached = avatarObjectUrls.get(uuid);
	if (cached) return cached;

	try {
		const response = await $axios.get(`/personas/${uuid}/avatar`, {
			responseType: 'blob',
		});
		const url = URL.createObjectURL(response.data);
		avatarObjectUrls.set(uuid, url);
		return url;
	} catch {
		return null;
	}
};

export const forgetPersonaAvatar = (uuid: string): void => {
	const url = avatarObjectUrls.get(uuid);
	if (url) URL.revokeObjectURL(url);
	avatarObjectUrls.delete(uuid);
};

const skillsKey = (personaUuid: string) => `personas-${personaUuid}-skills`;
const sharedSkillsKey = 'shared-skills';
const personaSharedSkillsKey = (personaUuid: string) =>
	`personas-${personaUuid}-shared-skills`;
const invalidateSharedLibrary = () => {
	requestCache.invalidate(/^shared-skills/);
	requestCache.invalidate(/^personas-.*-shared-skills$/);
	requestCache.invalidate(/^ws-\d+-persona-grants$/);
};
const invalidateSharedForPersona = (personaUuid: string) => {
	requestCache.invalidate(personaSharedSkillsKey(personaUuid));
	requestCache.invalidate(sharedSkillsKey);
};

const invalidateSharedPin = (personaUuid: string, slug: string) => {
	invalidateSharedForPersona(personaUuid);
	requestCache.invalidate(`shared-skills-${slug}-versions`);
	requestCache.invalidate(/^ws-\d+-persona-grants$/);
};

export const listPersonaSkills = async (
	personaUuid: string,
	useCache = true,
): Promise<PersonaSkillSummary[]> =>
	requestCache.getOrFetch(
		skillsKey(personaUuid),
		async () => {
			const {
				data: { data },
			} = await $axios.get(`/personas/${personaUuid}/skills`);
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);

export const getPersonaSkill = async (
	personaUuid: string,
	slug: string,
): Promise<PersonaSkill> => {
	const {
		data: { data },
	} = await $axios.get(`/personas/${personaUuid}/skills/${slug}`);
	return data;
};

export const putPersonaSkill = async (
	personaUuid: string,
	slug: string,
	body: string,
): Promise<PersonaSkill> => {
	const {
		data: { data },
	} = await $axios.put(`/personas/${personaUuid}/skills/${slug}`, { body });
	requestCache.invalidate(skillsKey(personaUuid));
	invalidateSharedForPersona(personaUuid);
	return data;
};

export const deletePersonaSkill = async (
	personaUuid: string,
	slug: string,
): Promise<void> => {
	await $axios.delete(`/personas/${personaUuid}/skills/${slug}`);
	requestCache.invalidate(skillsKey(personaUuid));
	invalidateSharedForPersona(personaUuid);
};

export interface ConvertPreviewPersona {
	id: string;
	name: string;
	archived: boolean;
	same_body: boolean;
	version: number;
}

export interface ConvertPreview {
	slug: string;
	library_clash: boolean;
	other_personas: ConvertPreviewPersona[];
}

export interface ConvertReplaceEntry {
	id: string;
	version: number;
}

export interface ConvertResult {
	skill: SharedSkill;
	replaced_personas: SharedSkillPersonaRef[];
}

export const previewConvertSkill = async (
	personaUuid: string,
	slug: string,
): Promise<ConvertPreview> => {
	const {
		data: { data },
	} = await $axios.get(`/personas/${personaUuid}/skills/${slug}/convert`);
	return data;
};

export const convertSkillToShared = async (
	personaUuid: string,
	slug: string,
	replacePersonas: ConvertReplaceEntry[],
): Promise<ConvertResult> => {
	const {
		data: { data },
	} = await $axios.post(`/personas/${personaUuid}/skills/${slug}/convert`, {
		replace_personas: replacePersonas,
	});
	invalidateSharedLibrary();
	requestCache.invalidate(skillsKey(personaUuid));
	for (const replaced of data.replaced_personas ?? []) {
		requestCache.invalidate(skillsKey(replaced.id));
	}
	return data;
};

export const listSharedSkills = async (
	useCache = true,
): Promise<SharedSkillSummary[]> =>
	requestCache.getOrFetch(
		sharedSkillsKey,
		async () => {
			const {
				data: { data },
			} = await $axios.get('/shared-skills');
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);

export const getSharedSkill = async (slug: string): Promise<SharedSkill> => {
	const {
		data: { data },
	} = await $axios.get(`/shared-skills/${slug}`);
	return data;
};

export const putSharedSkill = async (
	slug: string,
	body: string,
): Promise<SharedSkill> => {
	const {
		data: { data },
	} = await $axios.put(`/shared-skills/${slug}`, { body });
	invalidateSharedLibrary();
	return data;
};

export const deleteSharedSkill = async (
	slug: string,
): Promise<{ detached_personas: SharedSkillPersonaRef[] }> => {
	const {
		data: { data },
	} = await $axios.delete(`/shared-skills/${slug}`);
	invalidateSharedLibrary();
	return data;
};

export interface SharedSkillFile {
	path: string;
	size: number;
	sha256: string;
	updated_at: string;
}

export interface SharedSkillFileIndex {
	files: SharedSkillFile[];
	files_sha256: string;
}

export interface SharedSkillFileContent {
	path: string;
	content: string;
	size: number;
	sha256: string;
}

export const listSharedSkillFiles = async (
	slug: string,
): Promise<SharedSkillFileIndex> => {
	const {
		data: { data },
	} = await $axios.get(`/shared-skills/${slug}/files`);
	return data;
};

export const getSharedSkillFile = async (
	slug: string,
	path: string,
): Promise<SharedSkillFileContent> => {
	const {
		data: { data },
	} = await $axios.get(`/shared-skills/${slug}/files/content`, {
		params: { path },
	});
	return data;
};

export const deleteSharedSkillFile = async (
	slug: string,
	path: string,
	expectedFilesSha256: string,
): Promise<void> => {
	await $axios.delete(`/shared-skills/${slug}/files`, {
		params: { path, expected_files_sha256: expectedFilesSha256 },
	});
	invalidateSharedLibrary();
};

export const listSharedSkillVersions = async (
	slug: string,
	useCache = true,
): Promise<SharedSkillVersionRow[]> =>
	requestCache.getOrFetch(
		`shared-skills-${slug}-versions`,
		async () => {
			const {
				data: { data },
			} = await $axios.get(`/shared-skills/${slug}/versions`);
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);

export const getSharedSkillVersion = async (
	slug: string,
	version: number,
): Promise<SharedSkillVersion> => {
	const {
		data: { data },
	} = await $axios.get(`/shared-skills/${slug}/versions/${version}`);
	return data;
};

export const restoreSharedSkill = async (
	slug: string,
	version: number,
	expectedVersion: number,
): Promise<SharedSkill> => {
	const {
		data: { data },
	} = await $axios.post(`/shared-skills/${slug}/restore`, {
		version,
		expected_version: expectedVersion,
	});
	invalidateSharedLibrary();
	return data;
};

export const listDeletedSharedSkills = async (): Promise<
	DeletedSharedSkill[]
> => {
	const {
		data: { data },
	} = await $axios.get('/shared-skills/deleted');
	return data;
};

export const getDeletedSharedSkillVersion = async (
	slug: string,
	version: number,
): Promise<DeletedSharedSkillVersion> => {
	const {
		data: { data },
	} = await $axios.get(
		`/shared-skills/deleted/${encodeURIComponent(slug)}/versions/${version}`,
	);
	return data;
};

export const restoreDeletedSharedSkill = async (
	slug: string,
	version: number,
): Promise<SharedSkill> => {
	const {
		data: { data },
	} = await $axios.post(
		`/shared-skills/deleted/${encodeURIComponent(slug)}/restore`,
		{ version },
	);
	invalidateSharedLibrary();
	return data;
};

export const listPersonaSharedSkills = async (
	personaUuid: string,
	useCache = true,
): Promise<PersonaSharedSkill[]> =>
	requestCache.getOrFetch(
		personaSharedSkillsKey(personaUuid),
		async () => {
			const {
				data: { data },
			} = await $axios.get(`/personas/${personaUuid}/shared-skills`);
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);

export const attachSharedSkill = async (
	personaUuid: string,
	slug: string,
): Promise<PersonaSharedSkill> => {
	const {
		data: { data },
	} = await $axios.put(`/personas/${personaUuid}/shared-skills/${slug}`);
	invalidateSharedForPersona(personaUuid);
	return data;
};

export const detachSharedSkill = async (
	personaUuid: string,
	slug: string,
): Promise<void> => {
	await $axios.delete(`/personas/${personaUuid}/shared-skills/${slug}`);
	invalidateSharedForPersona(personaUuid);
};

export const pinSharedSkill = async (
	personaUuid: string,
	slug: string,
	version: number,
): Promise<PersonaSharedSkill> => {
	const {
		data: { data },
	} = await $axios.put(`/personas/${personaUuid}/shared-skills/${slug}/pin`, {
		version,
	});
	invalidateSharedPin(personaUuid, slug);
	return data;
};

export const unpinSharedSkill = async (
	personaUuid: string,
	slug: string,
): Promise<PersonaSharedSkill> => {
	const {
		data: { data },
	} = await $axios.delete(`/personas/${personaUuid}/shared-skills/${slug}/pin`);
	invalidateSharedPin(personaUuid, slug);
	return data;
};

const tokensKey = (personaUuid: string) => `personas-${personaUuid}-tokens`;

export const listPersonaTokens = async (
	personaUuid: string,
	useCache = true,
): Promise<PersonaToken[]> =>
	requestCache.getOrFetch(
		tokensKey(personaUuid),
		async () => {
			const {
				data: { data },
			} = await $axios.get(`/personas/${personaUuid}/tokens`);
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);

export const issuePersonaToken = async (
	personaUuid: string,
	payload: IssuePersonaTokenPayload,
): Promise<IssuedPersonaToken> => {
	const {
		data: { data },
	} = await $axios.post(`/personas/${personaUuid}/tokens`, payload);
	requestCache.invalidate(tokensKey(personaUuid));
	return data;
};

export const revokePersonaToken = async (
	tokenId: number,
	personaUuid: string,
): Promise<void> => {
	await $axios.delete(`/persona-tokens/${tokenId}`);
	requestCache.invalidate(tokensKey(personaUuid));
};

export const revokeAllPersonaTokens = async (
	personaUuid: string,
): Promise<void> => {
	await $axios.delete(`/personas/${personaUuid}/tokens`);
	requestCache.invalidate(tokensKey(personaUuid));
};

export const revokeAllMyPersonaTokens = async (): Promise<void> => {
	await $axios.delete('/persona-tokens');
	requestCache.invalidate(/^personas-.*-tokens$/);
};

export const listWorkspacePersonas = async (
	workspaceId: number,
	useCache = true,
): Promise<PersonaGrant[]> =>
	requestCache.getOrFetch(
		workspaceGrantsKey(workspaceId),
		async () => {
			const {
				data: { data },
			} = await $axios.get(`/workspaces/${workspaceId}/personas`);
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);

export const upsertWorkspaceGrant = async (
	workspaceId: number,
	personaUuid: string,
	permissions: string[],
): Promise<PersonaGrant> => {
	const {
		data: { data },
	} = await $axios.put(`/workspaces/${workspaceId}/personas/${personaUuid}`, {
		permissions,
	});
	requestCache.invalidate(workspaceGrantsKey(workspaceId));
	invalidateAssignable(workspaceId);
	return data;
};

export const removeWorkspaceGrant = async (
	workspaceId: number,
	personaUuid: string,
): Promise<void> => {
	await $axios.delete(`/workspaces/${workspaceId}/personas/${personaUuid}`);
	requestCache.invalidate(workspaceGrantsKey(workspaceId));
	invalidateAssignable(workspaceId);
};

export const blockWorkspacePersona = async (
	workspaceId: number,
	personaUuid: string,
): Promise<PersonaGrant> => {
	const {
		data: { data },
	} = await $axios.put(
		`/workspaces/${workspaceId}/personas/${personaUuid}/block`,
	);
	requestCache.invalidate(workspaceGrantsKey(workspaceId));
	invalidateAssignable(workspaceId);
	return data;
};

export const unblockWorkspacePersona = async (
	workspaceId: number,
	personaUuid: string,
): Promise<PersonaGrant> => {
	const {
		data: { data },
	} = await $axios.delete(
		`/workspaces/${workspaceId}/personas/${personaUuid}/block`,
	);
	requestCache.invalidate(workspaceGrantsKey(workspaceId));
	invalidateAssignable(workspaceId);
	return data;
};

export const getPersonaPolicy = async (
	workspaceId: number,
	useCache = true,
): Promise<PersonaPolicyResponse> =>
	requestCache.getOrFetch(
		workspacePolicyKey(workspaceId),
		async () => {
			const {
				data: { data },
			} = await $axios.get(`/workspaces/${workspaceId}/persona-policy`);
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);

export const setPersonaPolicy = async (
	workspaceId: number,
	policy: PersonaPolicy,
): Promise<PersonaPolicyResponse> => {
	const {
		data: { data },
	} = await $axios.put(`/workspaces/${workspaceId}/persona-policy`, {
		policy,
	});
	requestCache.invalidate(workspacePolicyKey(workspaceId));
	invalidateAssignable(workspaceId);
	return data;
};

export const getAssignablePersonas = async (
	workspaceId: number,
	useCache = true,
): Promise<PersonaAssignee[]> =>
	requestCache.getOrFetch(
		assignableKey(workspaceId),
		async () => {
			const {
				data: { data },
			} = await $axios.get(`/workspaces/${workspaceId}/assignable-personas`);
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);
