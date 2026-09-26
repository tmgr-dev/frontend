import axios, { type AxiosInstance } from 'axios';
import { createLocalApi } from './api';
import { dispatchLocal } from './dispatch';
import { respond } from './install';
import { localContext, localWorkspaceById } from './runtime';
import type { LocalUser } from './types';

const api = createLocalApi();

/**
 * An axios instance that answers from one local workspace's SQLite and has no network adapter at all.
 * Plugins get this instead of the app's client, so switching workspaces can never route their calls
 * to the cloud API or into another workspace.
 */
export const pinnedLocalClient = (
	workspaceId: number,
	currentUser: () => LocalUser,
): AxiosInstance =>
	axios.create({
		adapter: async (config) => {
			const workspace = await localWorkspaceById(workspaceId);
			if (!workspace)
				return respond(config, 409, { message: 'The local workspace is gone' });
			const ctx = await localContext(workspace, currentUser());
			const result = await dispatchLocal(
				api,
				ctx,
				config.method ?? 'get',
				config.url ?? '',
				config.data,
				config.params,
			);
			if (!result) {
				return respond(config, 501, {
					message: `Not available in local workspaces: ${(
						config.method ?? 'get'
					).toUpperCase()} ${config.url}`,
				});
			}
			return respond(config, result.status, result.data);
		},
	});
