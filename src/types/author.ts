export interface AuthorOwner {
	id: string | number;
	name: string;
}

/** Shared with plugins v1.1 (`TmgrCommentAuthor`) — comments, activity, reactions, realtime. */
export interface AuthorRef {
	kind: string;
	id: string;
	name: string;
	owner?: AuthorOwner;
	avatar?: string;
}
