<template>
  <div v-if="hasError" class="error-boundary">
    <div class="error-boundary-content">
      <div class="error-icon">
        <ExclamationTriangleIcon class="h-12 w-12 text-red-500" />
      </div>
      
      <div class="error-details">
        <h2 class="error-title">
          {{ errorTitle }}
        </h2>
        
        <p class="error-message">
          {{ errorMessage }}
        </p>
        
        <div v-if="showDetails && errorDetails" class="error-stack">
          <details class="mt-4">
            <summary class="cursor-pointer text-sm text-gray-600 dark:text-gray-400 hover:text-gray-800 dark:hover:text-gray-200">
              Show technical details
            </summary>
            <pre class="mt-2 text-xs bg-gray-100 dark:bg-gray-800 p-3 rounded overflow-auto max-h-40">{{ errorDetails }}</pre>
          </details>
        </div>
        
        <div class="error-actions">
          <Button
            @click="handleRetry"
            :disabled="retrying"
            class="mr-3"
          >
            <ArrowPathIcon 
              :class="[
                'h-4 w-4 mr-2',
                retrying && 'animate-spin'
              ]" 
            />
            {{ retrying ? 'Retrying...' : 'Try Again' }}
          </Button>
          
          <Button
            variant="outline"
            @click="handleReload"
          >
            <ArrowPathIcon class="h-4 w-4 mr-2" />
            Reload Page
          </Button>
          
          <Button
            v-if="showReportButton"
            variant="ghost"
            @click="handleReport"
            class="ml-3"
          >
            <ExclamationTriangleIcon class="h-4 w-4 mr-2" />
            Report Issue
          </Button>
        </div>
      </div>
    </div>
  </div>
  
  <slot v-else />
</template>

<script setup lang="ts">
import { ref, computed, onErrorCaptured, onMounted, onUnmounted } from 'vue';
import Button from '@/components/ui/button/Button.vue';
import { ExclamationTriangleIcon, ArrowPathIcon } from '@heroicons/vue/24/outline';

interface Props {
  fallbackTitle?: string;
  fallbackMessage?: string;
  showDetails?: boolean;
  showReportButton?: boolean;
  onError?: (error: Error, errorInfo: any) => void;
  onRetry?: () => void | Promise<void>;
}

const props = withDefaults(defineProps<Props>(), {
  fallbackTitle: 'Something went wrong',
  fallbackMessage: 'An unexpected error occurred. Please try refreshing the page.',
  showDetails: false,
  showReportButton: true
});

const emit = defineEmits<{
  'error': [error: Error, errorInfo: any];
  'retry': [];
  'report': [error: Error, errorInfo: any];
}>();

// Error state
const hasError = ref(false);
const error = ref<Error | null>(null);
const errorInfo = ref<any>(null);
const retrying = ref(false);

// Computed properties
const errorTitle = computed(() => {
  if (error.value?.name === 'ChunkLoadError') {
    return 'Update Available';
  }
  if (error.value?.name === 'NetworkError') {
    return 'Connection Problem';
  }
  return props.fallbackTitle;
});

const errorMessage = computed(() => {
  if (error.value?.name === 'ChunkLoadError') {
    return 'A new version of the application is available. Please refresh the page to get the latest updates.';
  }
  if (error.value?.name === 'NetworkError') {
    return 'Unable to connect to the server. Please check your internet connection and try again.';
  }
  if (error.value?.message) {
    return error.value.message;
  }
  return props.fallbackMessage;
});

const errorDetails = computed(() => {
  if (!error.value) return null;
  
  return {
    name: error.value.name,
    message: error.value.message,
    stack: error.value.stack,
    timestamp: new Date().toISOString(),
    userAgent: navigator.userAgent,
    url: window.location.href,
    ...errorInfo.value
  };
});

// Error capture
onErrorCaptured((err: Error, instance: any, info: string) => {
  console.error('Error boundary caught error:', err, info);
  
  hasError.value = true;
  error.value = err;
  errorInfo.value = {
    componentStack: info,
    componentInstance: instance?.$options?.name || 'Unknown'
  };
  
  // Call custom error handler
  if (props.onError) {
    props.onError(err, errorInfo.value);
  }
  
  // Emit error event
  emit('error', err, errorInfo.value);
  
  // Log to external service (if configured)
  logErrorToService(err, errorInfo.value);
  
  // Prevent the error from propagating
  return false;
});

// Global error handler for unhandled promise rejections
const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
  console.error('Unhandled promise rejection:', event.reason);
  
  const error = new Error(event.reason?.message || 'Unhandled promise rejection');
  error.name = 'UnhandledPromiseRejection';
  
  hasError.value = true;
  error.value = error;
  errorInfo.value = {
    type: 'unhandledRejection',
    reason: event.reason
  };
  
  // Prevent default browser behavior
  event.preventDefault();
};

// Global error handler for JavaScript errors
const handleGlobalError = (event: ErrorEvent) => {
  console.error('Global error:', event.error);
  
  hasError.value = true;
  error.value = event.error || new Error(event.message);
  errorInfo.value = {
    type: 'globalError',
    filename: event.filename,
    lineno: event.lineno,
    colno: event.colno
  };
};

// Setup global error handlers
onMounted(() => {
  window.addEventListener('unhandledrejection', handleUnhandledRejection);
  window.addEventListener('error', handleGlobalError);
});

// Cleanup global error handlers
onUnmounted(() => {
  window.removeEventListener('unhandledrejection', handleUnhandledRejection);
  window.removeEventListener('error', handleGlobalError);
});

// Action handlers
const handleRetry = async () => {
  retrying.value = true;
  
  try {
    // Call custom retry handler
    if (props.onRetry) {
      await props.onRetry();
    }
    
    // Reset error state
    hasError.value = false;
    error.value = null;
    errorInfo.value = null;
    
    emit('retry');
  } catch (retryError) {
    console.error('Retry failed:', retryError);
    // Keep error state, but update the error
    error.value = retryError instanceof Error ? retryError : new Error('Retry failed');
  } finally {
    retrying.value = false;
  }
};

const handleReload = () => {
  window.location.reload();
};

const handleReport = () => {
  if (error.value && errorInfo.value) {
    emit('report', error.value, errorInfo.value);
    
    // You could integrate with error reporting services here
    // Example: Sentry, Bugsnag, etc.
    reportErrorToService(error.value, errorInfo.value);
  }
};

// External error logging
const logErrorToService = (error: Error, info: any) => {
  // This would integrate with your error logging service
  // Example: Sentry, LogRocket, etc.
  try {
    if ((window as any).Sentry) {
      (window as any).Sentry.captureException(error, {
        contexts: {
          vue: info
        }
      });
    }
  } catch (loggingError) {
    console.error('Failed to log error to service:', loggingError);
  }
};

// Error reporting
const reportErrorToService = (error: Error, info: any) => {
  // This would send error reports to your support system
  try {
    const errorReport = {
      error: {
        name: error.name,
        message: error.message,
        stack: error.stack
      },
      info,
      timestamp: new Date().toISOString(),
      userAgent: navigator.userAgent,
      url: window.location.href
    };
    
    // Send to your error reporting endpoint
    fetch('/api/error-reports', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify(errorReport)
    }).catch(reportError => {
      console.error('Failed to send error report:', reportError);
    });
  } catch (reportingError) {
    console.error('Failed to create error report:', reportingError);
  }
};

// Expose methods for manual error handling
const captureError = (err: Error, info?: any) => {
  hasError.value = true;
  error.value = err;
  errorInfo.value = info || {};
  
  logErrorToService(err, info);
};

const clearError = () => {
  hasError.value = false;
  error.value = null;
  errorInfo.value = null;
};

// Expose methods to parent components
defineExpose({
  captureError,
  clearError,
  hasError: computed(() => hasError.value),
  error: computed(() => error.value)
});
</script>

<style scoped>
.error-boundary {
  @apply min-h-screen flex items-center justify-center p-4 bg-gray-50 dark:bg-gray-900;
}

.error-boundary-content {
  @apply max-w-md w-full bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6 text-center;
}

.error-icon {
  @apply mb-4;
}

.error-title {
  @apply text-xl font-semibold text-gray-900 dark:text-white mb-2;
}

.error-message {
  @apply text-gray-600 dark:text-gray-300 mb-6;
}

.error-actions {
  @apply flex flex-col sm:flex-row justify-center items-center gap-2;
}

.error-stack pre {
  @apply text-left;
}

/* Responsive adjustments */
@media (max-width: 640px) {
  .error-boundary-content {
    @apply p-4;
  }
  
  .error-title {
    @apply text-lg;
  }
  
  .error-actions {
    @apply flex-col;
  }
  
  .error-actions button {
    @apply w-full;
  }
}
</style>