import { ref, computed, onUnmounted, getCurrentInstance, type Ref, type ComputedRef } from 'vue';
import type { ActionError } from '@/types/dashboard';

export interface UseApiDataOptions<T> {
  immediate?: boolean;
  cache?: boolean;
  cacheKey?: string;
  cacheTtl?: number;
  retry?: number;
  retryDelay?: number;
  exponentialBackoff?: boolean;
  onSuccess?: (data: T) => void;
  onError?: (error: ActionError) => void;
  transform?: (data: T) => T;
}

export interface UseApiDataReturn<T> {
  data: Ref<T | null>;
  loading: Ref<boolean>;
  error: Ref<ActionError | null>;
  isError: ComputedRef<boolean>;
  isSuccess: ComputedRef<boolean>;
  isLoading: ComputedRef<boolean>;
  execute: () => Promise<T | null>;
  refresh: () => Promise<T | null>;
  reset: () => void;
  setData: (newData: T | null) => void;
}

const cache = new Map<string, { data: unknown; timestamp: number }>();

const DEFAULT_CACHE_TTL = 5 * 60 * 1000;

function getCachedData<T>(key: string, ttl: number): T | null {
  const cached = cache.get(key);
  if (!cached) return null;
  
  if (Date.now() - cached.timestamp > ttl) {
    cache.delete(key);
    return null;
  }
  
  return cached.data as T;
}

function setCachedData<T>(key: string, data: T): void {
  cache.set(key, { data, timestamp: Date.now() });
}

export function clearApiCache(keyPattern?: string): void {
  if (!keyPattern) {
    cache.clear();
    return;
  }
  
  for (const key of cache.keys()) {
    if (key.includes(keyPattern)) {
      cache.delete(key);
    }
  }
}

async function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

export function useApiData<T>(
  fetcher: () => Promise<T>,
  options: UseApiDataOptions<T> = {}
): UseApiDataReturn<T> {
  const {
    immediate = false,
    cache: useCache = false,
    cacheKey,
    cacheTtl = DEFAULT_CACHE_TTL,
    retry = 0,
    retryDelay = 1000,
    exponentialBackoff = true,
    onSuccess,
    onError,
    transform
  } = options;

  const data = ref<T | null>(null) as Ref<T | null>;
  const loading = ref(false);
  const error = ref<ActionError | null>(null);
  
  let abortController: AbortController | null = null;
  let retryCount = 0;

  const isError = computed(() => error.value !== null);
  const isSuccess = computed(() => data.value !== null && error.value === null);
  const isLoading = computed(() => loading.value);

  const createError = (err: unknown, type: ActionError['type'] = 'server'): ActionError => {
    if (typeof err === 'object' && err !== null && 'message' in err) {
      const errorObj = err as { message: string; code?: string | number; details?: unknown };
      return {
        message: errorObj.message,
        code: errorObj.code,
        details: errorObj.details,
        timestamp: new Date().toISOString(),
        type,
        recoverable: type !== 'authorization'
      };
    }
    
    return {
      message: String(err),
      timestamp: new Date().toISOString(),
      type,
      recoverable: true
    };
  };

  const execute = async (): Promise<T | null> => {
    if (abortController) {
      abortController.abort();
    }
    abortController = new AbortController();
    
    if (useCache && cacheKey) {
      const cachedData = getCachedData<T>(cacheKey, cacheTtl);
      if (cachedData) {
        data.value = cachedData;
        return cachedData;
      }
    }
    
    loading.value = true;
    error.value = null;
    retryCount = 0;
    
    const attemptFetch = async (): Promise<T> => {
      try {
        let result = await fetcher();
        
        if (transform) {
          result = transform(result);
        }
        
        return result;
      } catch (err) {
        if (retryCount < retry) {
          retryCount++;
          const delay = exponentialBackoff 
            ? retryDelay * Math.pow(2, retryCount - 1) 
            : retryDelay;
          
          await sleep(delay);
          return attemptFetch();
        }
        
        throw err;
      }
    };
    
    try {
      const result = await attemptFetch();
      
      data.value = result;
      
      if (useCache && cacheKey) {
        setCachedData(cacheKey, result);
      }
      
      if (onSuccess) {
        onSuccess(result);
      }
      
      return result;
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') {
        return null;
      }
      
      const actionError = createError(err);
      error.value = actionError;
      
      if (onError) {
        onError(actionError);
      }
      
      return null;
    } finally {
      loading.value = false;
    }
  };

  const refresh = async (): Promise<T | null> => {
    if (useCache && cacheKey) {
      cache.delete(cacheKey);
    }
    return execute();
  };

  const reset = (): void => {
    data.value = null;
    loading.value = false;
    error.value = null;
    retryCount = 0;
    
    if (abortController) {
      abortController.abort();
      abortController = null;
    }
  };

  const setData = (newData: T | null): void => {
    data.value = newData;
    error.value = null;
  };

  if (immediate) {
    execute();
  }

  if (getCurrentInstance()) {
    onUnmounted(() => {
      if (abortController) {
        abortController.abort();
      }
    });
  }

  return {
    data,
    loading,
    error,
    isError,
    isSuccess,
    isLoading,
    execute,
    refresh,
    reset,
    setData
  };
}
