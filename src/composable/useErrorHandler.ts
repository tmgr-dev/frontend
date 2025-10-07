import { ref, computed } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import type { ActionError } from '@/types/dashboard';

export interface ErrorState {
  error: ActionError | null;
  isRetrying: boolean;
  retryCount: number;
  maxRetries: number;
  lastRetryAt?: Date;
}

export interface RetryConfig {
  maxRetries?: number;
  retryDelay?: number;
  exponentialBackoff?: boolean;
  retryCondition?: (error: ActionError) => boolean;
}

export interface UseErrorHandlerReturn {
  // State
  error: Ref<ActionError | null>;
  errorState: Ref<ErrorState>;
  
  // Computed
  hasError: ComputedRef<boolean>;
  canRetry: ComputedRef<boolean>;
  isRetrying: ComputedRef<boolean>;
  errorMessage: ComputedRef<string>;
  errorType: ComputedRef<string>;
  
  // Actions
  setError: (error: ActionError | Error | string) => void;
  clearError: () => void;
  retry: (retryFn: () => Promise<void>) => Promise<boolean>;
  handleError: (error: any, context?: string) => ActionError;
  
  // Utilities
  isNetworkError: (error: ActionError) => boolean;
  isServerError: (error: ActionError) => boolean;
  isClientError: (error: ActionError) => boolean;
  isRecoverable: (error: ActionError) => boolean;
}

const defaultRetryConfig: Required<RetryConfig> = {
  maxRetries: 3,
  retryDelay: 1000,
  exponentialBackoff: true,
  retryCondition: (error: ActionError) => error.recoverable !== false
};

/**
 * Error handling composable for consistent error management
 * Provides error state management, retry logic, and error classification
 */
export function useErrorHandler(config: RetryConfig = {}): UseErrorHandlerReturn {
  const retryConfig = { ...defaultRetryConfig, ...config };
  
  // Error state
  const errorState = ref<ErrorState>({
    error: null,
    isRetrying: false,
    retryCount: 0,
    maxRetries: retryConfig.maxRetries
  });

  // Computed properties
  const error = computed(() => errorState.value.error);
  
  const hasError = computed(() => errorState.value.error !== null);
  
  const canRetry = computed(() => {
    const state = errorState.value;
    return state.error !== null && 
           state.retryCount < state.maxRetries && 
           !state.isRetrying &&
           retryConfig.retryCondition(state.error);
  });
  
  const isRetrying = computed(() => errorState.value.isRetrying);
  
  const errorMessage = computed(() => {
    const err = errorState.value.error;
    if (!err) return '';
    
    // Provide user-friendly messages for common error types
    switch (err.type) {
      case 'network':
        return 'Network connection failed. Please check your internet connection and try again.';
      case 'server':
        return err.message || 'Server error occurred. Please try again later.';
      case 'validation':
        return err.message || 'Invalid data provided. Please check your input.';
      case 'authentication':
        return 'Authentication failed. Please log in again.';
      case 'authorization':
        return 'You do not have permission to perform this action.';
      case 'not_found':
        return 'The requested resource was not found.';
      case 'timeout':
        return 'Request timed out. Please try again.';
      default:
        return err.message || 'An unexpected error occurred.';
    }
  });
  
  const errorType = computed(() => errorState.value.error?.type || 'unknown');

  // Error classification utilities
  const isNetworkError = (error: ActionError): boolean => {
    return error.type === 'network' || 
           error.message.toLowerCase().includes('network') ||
           error.message.toLowerCase().includes('fetch');
  };

  const isServerError = (error: ActionError): boolean => {
    return error.type === 'server' || 
           (error.status && error.status >= 500);
  };

  const isClientError = (error: ActionError): boolean => {
    return error.type === 'validation' || 
           error.type === 'authentication' || 
           error.type === 'authorization' ||
           (error.status && error.status >= 400 && error.status < 500);
  };

  const isRecoverable = (error: ActionError): boolean => {
    // Network errors and server errors are usually recoverable
    if (isNetworkError(error) || isServerError(error)) {
      return true;
    }
    
    // Authentication errors might be recoverable with re-login
    if (error.type === 'authentication') {
      return true;
    }
    
    // Check explicit recoverable flag
    return error.recoverable !== false;
  };

  // Error handling functions
  const normalizeError = (error: any): ActionError => {
    if (typeof error === 'string') {
      return {
        message: error,
        type: 'unknown',
        timestamp: new Date().toISOString(),
        recoverable: true
      };
    }
    
    if (error instanceof Error) {
      return {
        message: error.message,
        type: error.name === 'TypeError' ? 'network' : 'unknown',
        timestamp: new Date().toISOString(),
        recoverable: true,
        stack: error.stack
      };
    }
    
    if (error && typeof error === 'object') {
      return {
        message: error.message || 'Unknown error',
        type: error.type || 'unknown',
        timestamp: error.timestamp || new Date().toISOString(),
        recoverable: error.recoverable !== false,
        status: error.status,
        details: error.details,
        stack: error.stack
      };
    }
    
    return {
      message: 'Unknown error occurred',
      type: 'unknown',
      timestamp: new Date().toISOString(),
      recoverable: true
    };
  };

  const setError = (error: ActionError | Error | string): void => {
    const normalizedError = normalizeError(error);
    
    errorState.value = {
      ...errorState.value,
      error: normalizedError,
      isRetrying: false
    };
    
    // Log error for debugging
    console.error('Error set:', normalizedError);
  };

  const clearError = (): void => {
    errorState.value = {
      error: null,
      isRetrying: false,
      retryCount: 0,
      maxRetries: retryConfig.maxRetries
    };
  };

  const handleError = (error: any, context?: string): ActionError => {
    const normalizedError = normalizeError(error);
    
    // Add context if provided
    if (context) {
      normalizedError.context = context;
      normalizedError.message = `${context}: ${normalizedError.message}`;
    }
    
    setError(normalizedError);
    return normalizedError;
  };

  const retry = async (retryFn: () => Promise<void>): Promise<boolean> => {
    if (!canRetry.value) {
      return false;
    }
    
    errorState.value.isRetrying = true;
    errorState.value.retryCount++;
    errorState.value.lastRetryAt = new Date();
    
    try {
      // Calculate delay with exponential backoff
      const delay = retryConfig.exponentialBackoff
        ? retryConfig.retryDelay * Math.pow(2, errorState.value.retryCount - 1)
        : retryConfig.retryDelay;
      
      // Wait before retrying
      if (delay > 0) {
        await new Promise(resolve => setTimeout(resolve, delay));
      }
      
      // Execute retry function
      await retryFn();
      
      // Success - clear error
      clearError();
      return true;
      
    } catch (retryError) {
      // Retry failed - update error
      const normalizedError = normalizeError(retryError);
      errorState.value.error = normalizedError;
      errorState.value.isRetrying = false;
      
      console.error(`Retry ${errorState.value.retryCount} failed:`, normalizedError);
      return false;
    }
  };

  return {
    // State
    error,
    errorState,
    
    // Computed
    hasError,
    canRetry,
    isRetrying,
    errorMessage,
    errorType,
    
    // Actions
    setError,
    clearError,
    retry,
    handleError,
    
    // Utilities
    isNetworkError,
    isServerError,
    isClientError,
    isRecoverable
  };
}