export type SqlValue = string | number | null;

/** SQL access the local API needs: Rust commands in the app, node:sqlite in tests. */
export interface LocalDb {
	select<T = Record<string, any>>(sql: string, params?: SqlValue[]): Promise<T[]>;
	execute(
		sql: string,
		params?: SqlValue[],
	): Promise<{ rowsAffected: number; lastInsertId?: number }>;
}

export interface LocalWorkspace {
	id: number;
	name: string;
	code: string;
	schema_version: number;
	created_at: string;
	path: string;
	database: string;
}

export interface LocalUser {
	id: number;
	name: string;
	email: string;
}

export interface LocalContext {
	db: LocalDb;
	workspace: LocalWorkspace;
	user: LocalUser;
	now: () => Date;
}

export interface LocalRequest {
	method: string;
	path: string;
	params: Record<string, string>;
	query: URLSearchParams;
	body: any;
	ctx: LocalContext;
}

export interface LocalResponse {
	status: number;
	data: any;
}

export class LocalHttpError extends Error {
	constructor(
		public status: number,
		message: string,
	) {
		super(message);
	}
}

/** Workspace codes of local workspaces in URLs, never equal to a cloud workspace code. */
export const LOCAL_CODE_PREFIX = 'local-';
