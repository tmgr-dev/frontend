import { t } from '@/i18n';
import {
	CloudError,
	type MoveDeps,
	type MoveMode,
	type MoveTarget,
} from './batch';
import { firstActiveStatusId, type CloudApi } from './cloud';
import { buildRoutineRequest, buildTaskRequest } from './payload';

export const createMoveDeps = (
	api: CloudApi,
	removeLocal: (id: number) => Promise<void>,
): MoveDeps => ({
	create: (routine, mode, target, key) => {
		if (mode === 'routines') {
			const { path, body } = buildRoutineRequest(routine, target.workspaceId);
			return api.createRoutine(path, body);
		}
		return api.createTask(
			buildTaskRequest(routine, {
				workspaceId: target.workspaceId,
				categoryId: target.categoryId,
				statusId: target.statusId as number,
			}),
			key,
		);
	},
	removeLocal,
	addComment: (target, cloudId, message, key) =>
		api.addComment(target.workspaceId, cloudId, message, key),
});

export const prepareTarget = async (
	api: CloudApi,
	mode: MoveMode,
	workspaceId: number,
	categoryId: number | null,
): Promise<MoveTarget> => {
	if (mode === 'routines')
		return { workspaceId, categoryId: null, statusId: null };
	const statusId = firstActiveStatusId(await api.statuses(workspaceId));
	if (statusId === null)
		throw new CloudError(
			'rejected',
			t('dailyRoutine.moveToCloud.messages.noActiveStatus'),
		);
	return { workspaceId, categoryId, statusId };
};
