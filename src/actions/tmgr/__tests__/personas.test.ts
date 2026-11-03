jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		get: jest.fn(),
		post: jest.fn(),
		put: jest.fn(),
		delete: jest.fn(),
	},
}));

import axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import {
	archivePersona,
	attachSharedSkill,
	blockWorkspacePersona,
	convertSkillToShared,
	createPersona,
	deletePersonaSkill,
	deleteSharedSkill,
	detachSharedSkill,
	forgetPersonaAvatar,
	getAssignablePersonas,
	getPersonaPolicy,
	getPersonaSkill,
	getSharedSkill,
	issuePersonaToken,
	listPersonas,
	listPersonaSharedSkills,
	listPersonaSkills,
	listPersonaTokens,
	listSharedSkills,
	listWorkspacePersonas,
	personaAvatarObjectUrl,
	previewConvertSkill,
	putPersonaSkill,
	putSharedSkill,
	removeWorkspaceGrant,
	restorePersona,
	revokeAllMyPersonaTokens,
	revokeAllPersonaTokens,
	revokePersonaToken,
	setPersonaPolicy,
	unblockWorkspacePersona,
	updatePersona,
	uploadPersonaAvatar,
	upsertWorkspaceGrant,
} from '../personas';

const PERSONA = { id: 'uuid-1', name: 'Reviewer' };
const GRANT = { persona: PERSONA, workspace_id: 5, permissions: [] };

beforeEach(() => {
	requestCache.clear();
	jest.clearAllMocks();
});

describe('listPersonas', () => {
	it('GETs /personas and unwraps the envelope', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [PERSONA] } });
		const result = await listPersonas();
		expect(axios.get).toHaveBeenCalledWith('/personas', { params: undefined });
		expect(result).toEqual([PERSONA]);
	});

	it('passes archived=1 and caches separately from the default list', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [PERSONA] } });
		await listPersonas(true);
		expect(axios.get).toHaveBeenCalledWith('/personas', {
			params: { archived: 1 },
		});
	});
});

describe('createPersona', () => {
	it('POSTs the payload and invalidates the personas cache', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [PERSONA] } });
		await listPersonas();
		expect(requestCache.has('personas')).toBe(true);

		(axios.post as jest.Mock).mockResolvedValue({ data: { data: PERSONA } });
		const result = await createPersona({ name: 'Reviewer' });

		expect(axios.post).toHaveBeenCalledWith('/personas', { name: 'Reviewer' });
		expect(result).toEqual(PERSONA);
		expect(requestCache.has('personas')).toBe(false);
	});
});

describe('updatePersona', () => {
	it('PUTs to the persona URL and invalidates the cache', async () => {
		(axios.put as jest.Mock).mockResolvedValue({ data: { data: PERSONA } });
		const result = await updatePersona('uuid-1', { name: 'New name' });
		expect(axios.put).toHaveBeenCalledWith('/personas/uuid-1', {
			name: 'New name',
		});
		expect(result).toEqual(PERSONA);
	});
});

describe('archivePersona / restorePersona', () => {
	it('archives with DELETE', async () => {
		(axios.delete as jest.Mock).mockResolvedValue({});
		await archivePersona('uuid-1');
		expect(axios.delete).toHaveBeenCalledWith('/personas/uuid-1');
	});

	it('restores with POST', async () => {
		(axios.post as jest.Mock).mockResolvedValue({ data: { data: PERSONA } });
		const result = await restorePersona('uuid-1');
		expect(axios.post).toHaveBeenCalledWith('/personas/uuid-1/restore');
		expect(result).toEqual(PERSONA);
	});
});

describe('uploadPersonaAvatar', () => {
	it('uploads multipart form data and forgets the cached object URL', async () => {
		(axios.post as jest.Mock).mockResolvedValue({ data: { data: PERSONA } });
		const file = new File(['x'], 'avatar.png', { type: 'image/png' });
		const result = await uploadPersonaAvatar('uuid-1', file);

		expect(axios.post).toHaveBeenCalledWith(
			'/personas/uuid-1/avatar',
			expect.any(FormData),
			{ headers: { 'Content-Type': 'multipart/form-data' } },
		);
		expect(result).toEqual(PERSONA);
	});
});

describe('personaAvatarObjectUrl', () => {
	it('fetches the avatar as a blob and caches the object URL', async () => {
		const blob = new Blob(['x'], { type: 'image/png' });
		(axios.get as jest.Mock).mockResolvedValue({ data: blob });
		(global as any).URL.createObjectURL = jest.fn(() => 'blob:fake-url');
		(global as any).URL.revokeObjectURL = jest.fn();

		const first = await personaAvatarObjectUrl('uuid-1');
		const second = await personaAvatarObjectUrl('uuid-1');

		expect(axios.get).toHaveBeenCalledTimes(1);
		expect(axios.get).toHaveBeenCalledWith('/personas/uuid-1/avatar', {
			responseType: 'blob',
		});
		expect(first).toBe('blob:fake-url');
		expect(second).toBe('blob:fake-url');

		forgetPersonaAvatar('uuid-1');
		expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:fake-url');
	});

	it('returns null when the request fails', async () => {
		(axios.get as jest.Mock).mockRejectedValue(new Error('403'));
		const result = await personaAvatarObjectUrl('uuid-2');
		expect(result).toBeNull();
	});
});

describe('persona skills', () => {
	const SKILL = {
		slug: 'triage',
		title: 'Triage',
		when: 'sort it',
		actions: [],
		version: 1,
	};

	it('lists skills for a persona', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [SKILL] } });
		const result = await listPersonaSkills('uuid-1');
		expect(axios.get).toHaveBeenCalledWith('/personas/uuid-1/skills');
		expect(result).toEqual([SKILL]);
	});

	it('gets one skill with its body', async () => {
		const full = { ...SKILL, body: '---\n...' };
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: full } });
		const result = await getPersonaSkill('uuid-1', 'triage');
		expect(axios.get).toHaveBeenCalledWith('/personas/uuid-1/skills/triage');
		expect(result).toEqual(full);
	});

	it('puts a skill body and invalidates the skills cache', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [SKILL] } });
		await listPersonaSkills('uuid-1');
		expect(requestCache.has('personas-uuid-1-skills')).toBe(true);

		(axios.put as jest.Mock).mockResolvedValue({ data: { data: SKILL } });
		const result = await putPersonaSkill('uuid-1', 'triage', '---\nbody');

		expect(axios.put).toHaveBeenCalledWith('/personas/uuid-1/skills/triage', {
			body: '---\nbody',
		});
		expect(result).toEqual(SKILL);
		expect(requestCache.has('personas-uuid-1-skills')).toBe(false);
	});

	it('deletes a skill and invalidates the skills cache', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [SKILL] } });
		await listPersonaSkills('uuid-1');

		(axios.delete as jest.Mock).mockResolvedValue({});
		await deletePersonaSkill('uuid-1', 'triage');

		expect(axios.delete).toHaveBeenCalledWith('/personas/uuid-1/skills/triage');
		expect(requestCache.has('personas-uuid-1-skills')).toBe(false);
	});
});

describe('persona tokens', () => {
	const TOKEN = {
		id: 1,
		persona_id: 'uuid-1',
		workspace_id: 5,
		workspace_name: 'Demo',
		prefix: 'tmgrp_ab12',
		label: 'Claude Code',
		expires_at: '2026-04-01T00:00:00Z',
		last_used_at: null,
		revoked_at: null,
		created_at: '2026-01-01T00:00:00Z',
	};

	it('lists tokens for a persona', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [TOKEN] } });
		const result = await listPersonaTokens('uuid-1');
		expect(axios.get).toHaveBeenCalledWith('/personas/uuid-1/tokens');
		expect(result).toEqual([TOKEN]);
	});

	it("issues a token and invalidates that persona's token cache", async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [TOKEN] } });
		await listPersonaTokens('uuid-1');
		expect(requestCache.has('personas-uuid-1-tokens')).toBe(true);

		(axios.post as jest.Mock).mockResolvedValue({
			data: { data: { token: TOKEN, secret: 'tmgrp_secretvalue' } },
		});
		const result = await issuePersonaToken('uuid-1', {
			workspace_id: 5,
			label: 'Claude Code',
			expires_in_days: 90,
		});

		expect(axios.post).toHaveBeenCalledWith('/personas/uuid-1/tokens', {
			workspace_id: 5,
			label: 'Claude Code',
			expires_in_days: 90,
		});
		expect(result).toEqual({ token: TOKEN, secret: 'tmgrp_secretvalue' });
		expect(requestCache.has('personas-uuid-1-tokens')).toBe(false);
	});

	it("revokes one token and invalidates that persona's token cache", async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [TOKEN] } });
		await listPersonaTokens('uuid-1');

		(axios.delete as jest.Mock).mockResolvedValue({});
		await revokePersonaToken(1, 'uuid-1');

		expect(axios.delete).toHaveBeenCalledWith('/persona-tokens/1');
		expect(requestCache.has('personas-uuid-1-tokens')).toBe(false);
	});

	it('revokes all tokens for a persona', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [TOKEN] } });
		await listPersonaTokens('uuid-1');

		(axios.delete as jest.Mock).mockResolvedValue({});
		await revokeAllPersonaTokens('uuid-1');

		expect(axios.delete).toHaveBeenCalledWith('/personas/uuid-1/tokens');
		expect(requestCache.has('personas-uuid-1-tokens')).toBe(false);
	});

	it("revokes all of the owner's persona tokens across every persona", async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [TOKEN] } });
		await listPersonaTokens('uuid-1');
		await listPersonaTokens('uuid-2');

		(axios.delete as jest.Mock).mockResolvedValue({});
		await revokeAllMyPersonaTokens();

		expect(axios.delete).toHaveBeenCalledWith('/persona-tokens');
		expect(requestCache.has('personas-uuid-1-tokens')).toBe(false);
		expect(requestCache.has('personas-uuid-2-tokens')).toBe(false);
	});
});

describe('workspace grants', () => {
	it('lists grants for a workspace', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [GRANT] } });
		const result = await listWorkspacePersonas(5);
		expect(axios.get).toHaveBeenCalledWith('/workspaces/5/personas');
		expect(result).toEqual([GRANT]);
	});

	it('upserts a grant and invalidates that workspace only', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [GRANT] } });
		await listWorkspacePersonas(5);
		await listWorkspacePersonas(9);

		(axios.put as jest.Mock).mockResolvedValue({ data: { data: GRANT } });
		await upsertWorkspaceGrant(5, 'uuid-1', ['tasks:read']);

		expect(axios.put).toHaveBeenCalledWith('/workspaces/5/personas/uuid-1', {
			permissions: ['tasks:read'],
		});
		expect(requestCache.has('ws-5-persona-grants')).toBe(false);
		expect(requestCache.has('ws-9-persona-grants')).toBe(true);
	});

	it('removes a grant with DELETE', async () => {
		(axios.delete as jest.Mock).mockResolvedValue({});
		await removeWorkspaceGrant(5, 'uuid-1');
		expect(axios.delete).toHaveBeenCalledWith('/workspaces/5/personas/uuid-1');
	});

	it('blocks with PUT .../block and unblocks with DELETE .../block', async () => {
		(axios.put as jest.Mock).mockResolvedValue({ data: { data: GRANT } });
		await blockWorkspacePersona(5, 'uuid-1');
		expect(axios.put).toHaveBeenCalledWith(
			'/workspaces/5/personas/uuid-1/block',
		);

		(axios.delete as jest.Mock).mockResolvedValue({ data: { data: GRANT } });
		await unblockWorkspacePersona(5, 'uuid-1');
		expect(axios.delete).toHaveBeenCalledWith(
			'/workspaces/5/personas/uuid-1/block',
		);
	});
});

describe('persona policy', () => {
	it('reads the policy', async () => {
		(axios.get as jest.Mock).mockResolvedValue({
			data: { data: { policy: 'allowed', updated_at: 'now' } },
		});
		const result = await getPersonaPolicy(5);
		expect(axios.get).toHaveBeenCalledWith('/workspaces/5/persona-policy');
		expect(result.policy).toBe('allowed');
	});

	it('writes the policy and invalidates the cache', async () => {
		(axios.get as jest.Mock).mockResolvedValue({
			data: { data: { policy: 'allowed', updated_at: 'now' } },
		});
		await getPersonaPolicy(5);

		(axios.put as jest.Mock).mockResolvedValue({
			data: { data: { policy: 'read_only', updated_at: 'later' } },
		});
		await setPersonaPolicy(5, 'read_only');

		expect(axios.put).toHaveBeenCalledWith('/workspaces/5/persona-policy', {
			policy: 'read_only',
		});
		expect(requestCache.has('ws-5-persona-policy')).toBe(false);
	});
});

describe('getAssignablePersonas', () => {
	it('GETs the workspace endpoint and caches per workspace', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [PERSONA] } });
		const result = await getAssignablePersonas(5);
		await getAssignablePersonas(5);
		expect(axios.get).toHaveBeenCalledTimes(1);
		expect(axios.get).toHaveBeenCalledWith('/workspaces/5/assignable-personas');
		expect(result).toEqual([PERSONA]);
	});

	it.each([
		['creating a persona', () => createPersona({ name: 'X', workspace_id: 5 })],
		['upserting a grant', () => upsertWorkspaceGrant(5, 'uuid-1', [])],
		['removing a grant', () => removeWorkspaceGrant(5, 'uuid-1')],
		['blocking', () => blockWorkspacePersona(5, 'uuid-1')],
		['changing the policy', () => setPersonaPolicy(5, 'allowed')],
	])('is invalidated by %s', async (_name, mutate) => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [PERSONA] } });
		await getAssignablePersonas(5);
		expect(requestCache.has('personas-assignable-ws-5')).toBe(true);
		(axios.post as jest.Mock).mockResolvedValue({ data: { data: PERSONA } });
		(axios.put as jest.Mock).mockResolvedValue({ data: { data: GRANT } });
		(axios.delete as jest.Mock).mockResolvedValue({ data: { data: GRANT } });
		await mutate();
		expect(requestCache.has('personas-assignable-ws-5')).toBe(false);
	});

	it('sends workspace_id when creating a workspace persona', async () => {
		(axios.post as jest.Mock).mockResolvedValue({ data: { data: PERSONA } });
		await createPersona({ name: 'Shared', workspace_id: 5 });
		expect(axios.post).toHaveBeenCalledWith('/personas', {
			name: 'Shared',
			workspace_id: 5,
		});
	});
});

describe('shared skills library', () => {
	const SHARED = {
		slug: 'triage',
		title: 'Triage',
		when: 'sort it',
		actions: [],
		version: 1,
		updated_at: '2026-01-01T00:00:00Z',
		attached_personas: [{ id: 'uuid-1', name: 'Reviewer', overridden: false }],
	};

	const warm = async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [SHARED] } });
		await listSharedSkills();
		await listPersonaSharedSkills('uuid-1');
		await listPersonaSharedSkills('uuid-2');
		await listPersonaSkills('uuid-1');
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [GRANT] } });
		await listWorkspacePersonas(5);
		await listWorkspacePersonas(6);
	};

	it('lists the library and caches it under shared-skills', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [SHARED] } });
		const result = await listSharedSkills();
		expect(axios.get).toHaveBeenCalledWith('/shared-skills');
		expect(result).toEqual([SHARED]);
		expect(requestCache.has('shared-skills')).toBe(true);
	});

	it('gets one library skill with its body', async () => {
		const full = { ...SHARED, body: '---\n...' };
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: full } });
		const result = await getSharedSkill('triage');
		expect(axios.get).toHaveBeenCalledWith('/shared-skills/triage');
		expect(result).toEqual(full);
	});

	it('puts a library skill and invalidates library, persona panels and grants', async () => {
		await warm();
		(axios.put as jest.Mock).mockResolvedValue({ data: { data: SHARED } });
		const result = await putSharedSkill('triage', '---\nbody');

		expect(axios.put).toHaveBeenCalledWith('/shared-skills/triage', {
			body: '---\nbody',
		});
		expect(result).toEqual(SHARED);
		expect(requestCache.has('shared-skills')).toBe(false);
		expect(requestCache.has('personas-uuid-1-shared-skills')).toBe(false);
		expect(requestCache.has('personas-uuid-2-shared-skills')).toBe(false);
		expect(requestCache.has('ws-5-persona-grants')).toBe(false);
		expect(requestCache.has('ws-6-persona-grants')).toBe(false);
		expect(requestCache.has('personas-uuid-1-skills')).toBe(true);
	});

	it('deletes a library skill, returns the detached personas and invalidates the same keys', async () => {
		await warm();
		const detached = [{ id: 'uuid-1', name: 'Reviewer' }];
		(axios.delete as jest.Mock).mockResolvedValue({
			data: { data: { detached_personas: detached } },
		});
		const result = await deleteSharedSkill('triage');

		expect(axios.delete).toHaveBeenCalledWith('/shared-skills/triage');
		expect(result).toEqual({ detached_personas: detached });
		expect(requestCache.has('shared-skills')).toBe(false);
		expect(requestCache.has('personas-uuid-2-shared-skills')).toBe(false);
		expect(requestCache.has('ws-5-persona-grants')).toBe(false);
	});

	it('lists the skills attached to a persona', async () => {
		const entry = { ...SHARED, overridden: true };
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [entry] } });
		const result = await listPersonaSharedSkills('uuid-1');
		expect(axios.get).toHaveBeenCalledWith('/personas/uuid-1/shared-skills');
		expect(result).toEqual([entry]);
		expect(requestCache.has('personas-uuid-1-shared-skills')).toBe(true);
	});

	it('attaches and detaches, invalidating only that persona and the library', async () => {
		await warm();
		(axios.put as jest.Mock).mockResolvedValue({ data: { data: SHARED } });
		const result = await attachSharedSkill('uuid-1', 'triage');

		expect(axios.put).toHaveBeenCalledWith(
			'/personas/uuid-1/shared-skills/triage',
		);
		expect(result).toEqual(SHARED);
		expect(requestCache.has('personas-uuid-1-shared-skills')).toBe(false);
		expect(requestCache.has('shared-skills')).toBe(false);
		expect(requestCache.has('personas-uuid-2-shared-skills')).toBe(true);
		expect(requestCache.has('ws-5-persona-grants')).toBe(true);

		await listSharedSkills();
		await listPersonaSharedSkills('uuid-1');
		(axios.delete as jest.Mock).mockResolvedValue({});
		await detachSharedSkill('uuid-1', 'triage');

		expect(axios.delete).toHaveBeenCalledWith(
			'/personas/uuid-1/shared-skills/triage',
		);
		expect(requestCache.has('personas-uuid-1-shared-skills')).toBe(false);
		expect(requestCache.has('shared-skills')).toBe(false);
		expect(requestCache.has('personas-uuid-2-shared-skills')).toBe(true);
	});

	it('refreshes the override flags when an own skill is saved or deleted', async () => {
		await warm();
		(axios.put as jest.Mock).mockResolvedValue({ data: { data: {} } });
		await putPersonaSkill('uuid-1', 'triage', 'x');
		expect(requestCache.has('personas-uuid-1-shared-skills')).toBe(false);
		expect(requestCache.has('shared-skills')).toBe(false);
		expect(requestCache.has('personas-uuid-2-shared-skills')).toBe(true);

		await warm();
		(axios.delete as jest.Mock).mockResolvedValue({});
		await deletePersonaSkill('uuid-1', 'triage');
		expect(requestCache.has('personas-uuid-1-shared-skills')).toBe(false);
		expect(requestCache.has('shared-skills')).toBe(false);
	});
});

describe('convert own skill to shared', () => {
	const PREVIEW = {
		slug: 'triage',
		library_clash: false,
		other_personas: [
			{
				id: 'uuid-2',
				name: 'Helper',
				archived: false,
				same_body: true,
				version: 3,
			},
		],
	};

	it('previews the conversion without caching it', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: PREVIEW } });
		const result = await previewConvertSkill('uuid-1', 'triage');
		expect(axios.get).toHaveBeenCalledWith(
			'/personas/uuid-1/skills/triage/convert',
		);
		expect(result).toEqual(PREVIEW);
		await previewConvertSkill('uuid-1', 'triage');
		expect(axios.get).toHaveBeenCalledTimes(2);
	});

	it('posts the replace list and invalidates library, shared, own skills and grants', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [] } });
		await listSharedSkills();
		await listPersonaSharedSkills('uuid-1');
		await listPersonaSharedSkills('uuid-3');
		await listPersonaSkills('uuid-1');
		await listPersonaSkills('uuid-2');
		await listPersonaSkills('uuid-3');
		await listWorkspacePersonas(5);
		const response = {
			skill: { slug: 'triage' },
			replaced_personas: [{ id: 'uuid-2', name: 'Helper' }],
		};
		(axios.post as jest.Mock).mockResolvedValue({ data: { data: response } });

		const result = await convertSkillToShared('uuid-1', 'triage', [
			{ id: 'uuid-2', version: 3 },
		]);

		expect(axios.post).toHaveBeenCalledWith(
			'/personas/uuid-1/skills/triage/convert',
			{ replace_personas: [{ id: 'uuid-2', version: 3 }] },
		);
		expect(result).toEqual(response);
		expect(requestCache.has('shared-skills')).toBe(false);
		expect(requestCache.has('personas-uuid-1-shared-skills')).toBe(false);
		expect(requestCache.has('personas-uuid-3-shared-skills')).toBe(false);
		expect(requestCache.has('personas-uuid-1-skills')).toBe(false);
		expect(requestCache.has('personas-uuid-2-skills')).toBe(false);
		expect(requestCache.has('personas-uuid-3-skills')).toBe(true);
		expect(requestCache.has('ws-5-persona-grants')).toBe(false);
	});

	it('keeps caches when the server rejects the conversion', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [] } });
		await listSharedSkills();
		(axios.post as jest.Mock).mockRejectedValue(new Error('409'));
		await expect(
			convertSkillToShared('uuid-1', 'triage', []),
		).rejects.toThrow();
		expect(requestCache.has('shared-skills')).toBe(true);
	});
});
