export interface AuthorOwner {
	id: string | number;
	name: string;
}

export interface AuthorRef {
	kind: string;
	id: string;
	name: string;
	owner?: AuthorOwner;
	avatar?: string;
}
