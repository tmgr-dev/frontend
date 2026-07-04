// API base URL — same backend as the web app. Override via EXPO_PUBLIC_API_BASE_URL.
export const API_BASE_URL =
	process.env.EXPO_PUBLIC_API_BASE_URL ??
	'https://tmgr-api-stage.k8s.in-the.dev/api/';
