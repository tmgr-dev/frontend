export interface Time {
	hours: number;
	minutes: number;
	seconds?: number;
}

export interface ExtendedTime extends Time {
	timeLeft: string;
}

export type EditorType = 'markdown' | 'block';

// Export all dashboard types
export * from './dashboard';
