import { Task } from '@/actions/tmgr/tasks';

export const getBlockEditorDescription = (description: Task['description']) => {
	return {
		time: 1731156298664,
		blocks: [
			{
				id: 'Sb8IPG_R7P',
				type: 'paragraph',
				data: {
					text: description,
				},
			},
		],
		version: '2.30.6',
	};
};
