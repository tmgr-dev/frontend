import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import type { PersonaAssignee, PersonaPolicy } from '@/utils/personas';

export interface PersonaOwnerRef {
	id: number;
	name: string;
}

export interface Persona {
	id: string;
	name: string;
	description: string | null;
	avatar_url: string | null;
	system_prompt?: string;
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

export const createPersona = async (payload: PersonaInput): Promise<Persona> => {
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
	return data;
};

export const deletePersonaSkill = async (
	personaUuid: string,
	slug: string,
): Promise<void> => {
	await $axios.delete(`/personas/${personaUuid}/skills/${slug}`);
	requestCache.invalidate(skillsKey(personaUuid));
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
	} = await $axios.put(
		`/workspaces/${workspaceId}/personas/${personaUuid}`,
		{ permissions },
	);
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
