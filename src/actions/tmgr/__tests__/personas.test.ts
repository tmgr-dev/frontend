jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn(), put: jest.fn(), delete: jest.fn() },
}));

import axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import {
	archivePersona,
	blockWorkspacePersona,
	createPersona,
	forgetPersonaAvatar,
	getPersonaPolicy,
	listPersonas,
	listWorkspacePersonas,
	personaAvatarObjectUrl,
	removeWorkspaceGrant,
	restorePersona,
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
		expect(axios.get).toHaveBeenCalledWith('/personas', { params: { archived: 1 } });
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
