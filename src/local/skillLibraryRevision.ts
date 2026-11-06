import { ref } from 'vue';

export const skillLibraryRevision = ref(0);

export const bumpSkillLibraryRevision = () => {
	skillLibraryRevision.value += 1;
};
