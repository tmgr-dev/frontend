import { reactive } from 'vue';

export const moveUi = reactive({
	dialogCode: null as string | null,
	progressCode: null as string | null,
});

export const openMoveDialog = (code: string) => {
	moveUi.dialogCode = code;
};

export const openMoveProgress = (code: string) => {
	moveUi.progressCode = code;
};
