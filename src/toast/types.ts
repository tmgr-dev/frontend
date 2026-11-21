export type ToastKind = 'decision' | 'agent' | 'idle' | 'limit';

export type ToastAction = { id: string; label: string; primary?: boolean };

export type ToastView = {
	id: string;
	kind: ToastKind;
	title: string;
	body: string;
	meta?: string;
	actions: ToastAction[];
	at: number;
};

export type ToastList = {
	visible: ToastView[];
	more: number;
	expanded: boolean;
};
