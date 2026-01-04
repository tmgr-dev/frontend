import $axios from '@/plugins/axios';

export interface FeatureToggle {
    key: string;
    name: string;
    group: string;
    enabled?: boolean;
    value?: string | boolean;
    default?: string | boolean;
    type?: 'boolean' | 'select';
    options?: string[];
}

export const getWorkspaceFeatureToggles = async (workspaceId: number): Promise<Record<string, FeatureToggle>> => {
    const { data: { data } } = await $axios.get(`/workspaces/${workspaceId}/feature-toggles`);
    return data;
};

export const updateWorkspaceFeatureToggles = async (
    workspaceId: number, 
    features: Record<string, boolean>
): Promise<void> => {
    await $axios.put(`/workspaces/${workspaceId}/feature-toggles`, { features });
};

export const getUserFeatureToggles = async (): Promise<Record<string, FeatureToggle>> => {
    const { data: { data } } = await $axios.get('/user/feature-toggles');
    return data;
};

export const updateUserFeatureToggles = async (
    features: Record<string, boolean | string>
): Promise<void> => {
    await $axios.put('/user/feature-toggles', { features });
};

