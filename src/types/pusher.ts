import type { Task } from '@/actions/tmgr/tasks';

export interface PusherChannel {
	listen(event: string, callback: PusherEventCallback): PusherChannel;
	stopListening(event: string): PusherChannel;
	subscribed(callback: () => void): PusherChannel;
	error(callback: (error: PusherError) => void): PusherChannel;
}

export interface PusherError {
	type: string;
	error: {
		type: string;
		data?: {
			code?: number;
			message?: string;
		};
	};
}

export type PusherEventCallback = (data: PusherEventData) => void;

export interface PusherEventData {
	[key: string]: unknown;
}

export interface TaskCountdownEvent {
	taskId: number;
	action: 'start' | 'stop';
	startTime: number | null;
	commonTime: number;
}

export interface TaskUpdatedEvent {
	task: Task;
	action: 'created' | 'updated' | 'deleted';
}

export interface TaskStatusChangedEvent {
	taskId: number;
	previousStatusId: number;
	newStatusId: number;
}

export interface WorkspaceUpdatedEvent {
	workspaceId: number;
	action: 'updated' | 'member_added' | 'member_removed';
	userId?: number;
}

export interface NotificationEvent {
	id: number;
	type: string;
	title: string;
	message: string | null;
	data: Record<string, unknown> | null;
	createdAt: string;
}

export interface ActivityEvent {
	id: number;
	type: string;
	description: string;
	userId: number;
	taskId?: number;
	createdAt: string;
}

export type PusherEventType =
	| 'TaskCountdownStarted'
	| 'TaskCountdownStopped'
	| 'TaskUpdated'
	| 'TaskCreated'
	| 'TaskDeleted'
	| 'TaskStatusChanged'
	| 'WorkspaceUpdated'
	| 'NotificationReceived'
	| 'ActivityCreated';

export interface PusherEventMap {
	TaskCountdownStarted: TaskCountdownEvent;
	TaskCountdownStopped: TaskCountdownEvent;
	TaskUpdated: TaskUpdatedEvent;
	TaskCreated: TaskUpdatedEvent;
	TaskDeleted: TaskUpdatedEvent;
	TaskStatusChanged: TaskStatusChangedEvent;
	WorkspaceUpdated: WorkspaceUpdatedEvent;
	NotificationReceived: NotificationEvent;
	ActivityCreated: ActivityEvent;
}
