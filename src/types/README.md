# Dashboard API Response Type Definitions

This document describes the comprehensive API response type definitions implemented for the dashboard analytics feature.

## Overview

The dashboard types provide complete TypeScript definitions for:
- API response structures
- Error handling
- Loading states
- Filtering and sorting options
- Request parameters
- Type guards for runtime validation

## Key Features Implemented

### 1. Paginated Response Types

```typescript
interface PaginatedResponse<T = any> {
  data: T[];
  meta: PaginationMeta;
  links: PaginationLinks;
}

interface PaginationMeta {
  current_page: number;
  last_page: number;
  per_page: number;
  total: number;
  from: number;
  to: number;
  // ... additional pagination metadata
}
```

### 2. Error Response Interfaces

#### Generic API Error Response
```typescript
interface ApiErrorResponse {
  error: string;
  message: string;
  errors?: Record<string, string[]>;
  status_code: number;
  timestamp?: string;
  trace_id?: string;
  request_id?: string;
}
```

#### Validation Error Response
```typescript
interface ValidationErrorResponse extends ApiErrorResponse {
  errors: Record<string, string[]>;
}
```

#### Network Error Response
```typescript
interface NetworkErrorResponse {
  error: 'network_error';
  message: string;
  status_code: 0;
  timestamp: string;
  is_offline?: boolean;
  retry_after?: number;
}
```

### 3. Loading State Type Definitions

#### Basic Loading State
```typescript
interface LoadingState {
  isLoading: boolean;
  error?: string | null;
  lastUpdated?: string;
  progress?: number;
  retryCount?: number;
  canRetry?: boolean;
}
```

#### Enhanced Loading State
```typescript
interface DetailedLoadingState extends LoadingState {
  status: 'idle' | 'loading' | 'success' | 'error' | 'retrying';
  startTime?: number;
  endTime?: number;
  duration?: number;
}
```

#### Dashboard-Specific Loading States
```typescript
interface DashboardLoadingStates {
  statistics: DetailedLoadingState;
  activities: DetailedLoadingState;
  heatmap: DetailedLoadingState;
  recentTasks: DetailedLoadingState;
  teamActivity: DetailedLoadingState;
  loadingMore: boolean;
  initialLoad: boolean;
  refreshing: boolean;
}
```

### 4. Filter and Sort Option Types

#### Activity Filters
```typescript
interface ActivityFilters {
  type?: ActivityType | 'all';
  user_id?: number | 'all';
  date_from?: string;
  date_to?: string;
  subject_type?: string;
  search?: string;
  limit?: number;
}

interface AdvancedActivityFilters extends ActivityFilters {
  exclude_types?: ActivityType[];
  include_metadata?: boolean;
  group_by?: 'date' | 'user' | 'type';
  time_range?: 'today' | 'week' | 'month' | 'year' | 'custom';
}
```

#### Task Filters
```typescript
interface TaskFilters {
  status_id?: number | 'all';
  category_id?: number | 'all';
  user_id?: number | 'all';
  priority?: 'low' | 'medium' | 'high' | 'urgent' | 'all';
  is_overdue?: boolean;
  has_timer?: boolean;
  search?: string;
  assigned_to_me?: boolean;
  created_by_me?: boolean;
}
```

#### Sort Options
```typescript
interface SortOption {
  field: string;
  direction: 'asc' | 'desc';
}

type ActivitySortField = 'created_at' | 'type' | 'user_name';
type TaskSortField = 'updated_at' | 'created_at' | 'title' | 'priority' | 'status';

interface TypedSortOption<T extends string> {
  field: T;
  direction: 'asc' | 'desc';
}
```

### 5. Action Result Types

```typescript
interface ActionResult<T = any> {
  data?: T;
  error?: ActionError;
  success: boolean;
  status_code?: number;
  cached?: boolean;
  duration?: number;
  timestamp: string;
}

interface DetailedActionResult<T = any> extends ActionResult<T> {
  status: ResponseStatus;
  metadata?: {
    request_id?: string;
    trace_id?: string;
    cache_key?: string;
    retry_count?: number;
    source?: 'cache' | 'network' | 'fallback';
  };
}
```

### 6. Specific Error Types

```typescript
interface NetworkError extends ActionError {
  type: 'network';
  is_offline?: boolean;
  status_code: number;
}

interface ValidationError extends ActionError {
  type: 'validation';
  field_errors: Record<string, string[]>;
}

interface AuthorizationError extends ActionError {
  type: 'authorization';
  required_permission?: string;
  current_role?: string;
}

interface ServerError extends ActionError {
  type: 'server';
  status_code: number;
  trace_id?: string;
}

interface TimeoutError extends ActionError {
  type: 'timeout';
  timeout_duration: number;
  operation: string;
}
```

### 7. Type Guards for Runtime Validation

```typescript
// Validate API error responses
export const isApiErrorResponse = (obj: any): obj is ApiErrorResponse => {
  return obj &&
    typeof obj.error === 'string' &&
    typeof obj.message === 'string' &&
    typeof obj.status_code === 'number';
};

// Validate validation error responses
export const isValidationErrorResponse = (obj: any): obj is ValidationErrorResponse => {
  return isApiErrorResponse(obj) &&
    obj.errors &&
    typeof obj.errors === 'object';
};

// Validate network error responses
export const isNetworkErrorResponse = (obj: any): obj is NetworkErrorResponse => {
  return obj &&
    obj.error === 'network_error' &&
    typeof obj.message === 'string' &&
    obj.status_code === 0;
};

// Validate paginated responses
export const isPaginatedResponse = <T>(obj: any): obj is PaginatedResponse<T> => {
  return obj &&
    Array.isArray(obj.data) &&
    obj.meta &&
    typeof obj.meta.current_page === 'number' &&
    typeof obj.meta.total === 'number';
};
```

### 8. Request Parameter Types

```typescript
interface BaseRequestParams {
  workspace_id?: number;
  include_deleted?: boolean;
  include_metadata?: boolean;
  timezone?: string;
}

interface PaginatedRequestParams extends BaseRequestParams {
  page?: number;
  per_page?: number;
  cursor?: string;
}

interface FilteredRequestParams<TFilter = any, TSort = SortOption> extends PaginatedRequestParams {
  filters?: TFilter;
  sort?: TSort | TSort[];
  search?: string;
}
```

### 9. Dashboard-Specific Response Types

```typescript
export interface DashboardStatisticsResponse extends ApiResponse<DashboardStatistics> {}
export interface ActivityFeedResponse extends ApiResponse<PaginatedResponse<Activity>> {}
export interface HeatmapDataResponse extends ApiResponse<HeatmapData> {}
export interface RecentTasksResponse extends ApiResponse<RecentTask[]> {}
export interface TeamActivityResponse extends ApiResponse<TeamMemberActivity> {}
```

## Usage Examples

See `examples/dashboard-api-usage.ts` for comprehensive usage examples including:

1. **API Response Handling**: Proper typing and validation of API responses
2. **Error Handling**: Type-safe error handling with specific error types
3. **Loading State Management**: Complete loading state management system
4. **API Client Implementation**: Full API client with proper error handling and type safety
5. **Filter and Sort Usage**: Examples of using filters and sorting options

## Benefits

1. **Type Safety**: Complete compile-time type checking for all API interactions
2. **Runtime Validation**: Type guards provide runtime validation of API responses
3. **Error Handling**: Comprehensive error types for different failure scenarios
4. **Developer Experience**: IntelliSense support and auto-completion for all API interactions
5. **Maintainability**: Clear interfaces make the codebase easier to maintain and extend
6. **Testing**: Well-defined types make unit testing easier and more reliable

## Requirements Satisfied

This implementation satisfies the following requirements from the task:

- ✅ **Create paginated response types**: Comprehensive pagination support with metadata and links
- ✅ **Define error response interfaces**: Multiple error types for different scenarios
- ✅ **Add loading state type definitions**: Basic and enhanced loading states with progress tracking
- ✅ **Include filter and sort option types**: Complete filtering and sorting type definitions
- ✅ **Requirements 10.1, 10.2**: Error handling and loading states as specified in the requirements

## Files Created/Modified

1. **`src/types/dashboard.ts`**: Enhanced with comprehensive API response types
2. **`src/types/__tests__/dashboard-types.test.ts`**: Test file for type validation
3. **`src/types/examples/dashboard-api-usage.ts`**: Usage examples and API client implementation
4. **`src/types/README.md`**: This documentation file

All types are fully compatible with the existing dashboard implementation and provide a solid foundation for the frontend API integration.