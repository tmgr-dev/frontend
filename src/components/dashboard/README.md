# Dashboard Components

This directory contains all the Vue components for the dashboard analytics feature.

## Components Overview

### StatisticsGrid.vue
Main container for displaying workspace statistics in a grid layout.
- Displays key metrics like total tasks, active tasks, completed tasks, time tracking, etc.
- Supports loading states with skeleton loaders
- Includes click handlers for navigation to filtered views
- Fully accessible with ARIA labels

### StatisticCard.vue
Individual statistic card component used within StatisticsGrid.
- Shows icon, title, value, and optional trend information
- Supports different color themes (blue, green, orange, red, purple, indigo)
- Includes hover effects and loading states
- Accessible with proper ARIA attributes

### ActivityFeed.vue
Displays a chronological feed of workspace activities.
- Shows paginated list of activities with real-time updates via Pusher
- Includes filtering and search functionality
- Supports "load more" pagination
- Real-time connection status indicator

### ActivityItem.vue
Individual activity item within the activity feed.
- Shows user avatar, activity description, timestamp
- Displays activity metadata (task titles, status changes, etc.)
- Color-coded icons based on activity type
- Clickable for detailed views

### ActivityFilters.vue
Filter controls for the activity feed.
- Filter by activity type, user, and date range
- Dropdown selectors with predefined options
- Clear filters functionality
- Responsive design for mobile

### RecentTasksList.vue
Widget showing recently updated tasks.
- Displays 5-10 most recent tasks
- Shows task status, category, assignees, and update time
- Includes timer indicators for running timers
- Quick actions for timer control and status changes

### RecentTaskItem.vue
Individual task item within the recent tasks list.
- Shows task title, status, category, priority
- Displays assignee avatars and timer status
- Inline status change dropdown
- Timer start/stop functionality with live duration display

### TeamActivityWidget.vue
Shows team member presence and current activities.
- Lists all workspace members with online/offline status
- Shows current tasks and running timers
- Team summary statistics (total members, online count, active timers)
- Real-time status updates

### TeamMemberItem.vue
Individual team member item within the team activity widget.
- Member avatar with online status indicator
- Current task information with timer duration
- Last activity timestamp
- Clickable for member profile navigation

## Skeleton Components

Each main component has a corresponding skeleton component for loading states:
- `ActivityItemSkeleton.vue`
- `RecentTaskSkeleton.vue`
- `TeamMemberSkeleton.vue`

## Usage

```vue
<template>
  <div class="dashboard">
    <StatisticsGrid 
      :statistics="dashboardStats" 
      :loading="statsLoading"
      @navigate-to-filtered="handleNavigation"
    />
    
    <div class="dashboard-grid">
      <ActivityFeed
        :activities="activities"
        :loading="activitiesLoading"
        :workspace-id="currentWorkspaceId"
        @load-more="loadMoreActivities"
        @refresh="refreshActivities"
      />
      
      <div class="sidebar">
        <RecentTasksList
          :tasks="recentTasks"
          :loading="tasksLoading"
          @task-click="openTaskModal"
        />
        
        <TeamActivityWidget
          :team-activity="teamActivity"
          :loading="teamLoading"
          @member-click="viewMemberProfile"
        />
      </div>
    </div>
  </div>
</template>
```

## Features

- **Real-time Updates**: All components support real-time updates via Pusher
- **Loading States**: Skeleton loaders for better UX during data loading
- **Accessibility**: Full keyboard navigation and screen reader support
- **Responsive Design**: Mobile-friendly layouts and interactions
- **Dark Mode**: Full dark mode support with proper color schemes
- **Interactive Elements**: Click handlers, hover effects, and smooth animations
- **Error Handling**: Graceful error states and retry mechanisms

## Dependencies

- Vue 3 with Composition API
- TypeScript for type safety
- Tailwind CSS for styling
- Heroicons for icons
- Radix Vue for UI primitives
- Custom UI components (Button, Select, Skeleton, etc.)

## Type Safety

All components are fully typed with TypeScript interfaces defined in `@/types/dashboard.ts`.