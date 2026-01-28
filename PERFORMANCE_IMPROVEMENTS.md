# Performance Improvements

Этот документ описывает оптимизации производительности, внедренные в проект.

## 1. Request Caching (`src/utils/requestCache.ts`)

### Использование

```typescript
import { requestCache } from '@/utils/requestCache';

// Генерация ключа кэша
const cacheKey = requestCache.generateKey('endpoint', { params });

// Получение из кэша
const cached = requestCache.get<DataType>(cacheKey);
if (cached) {
  return cached;
}

// Сохранение в кэш (TTL по умолчанию 5 минут)
requestCache.set(cacheKey, data);

// Сохранение с кастомным TTL (в миллисекундах)
requestCache.set(cacheKey, data, 60000); // 1 минута

// Инвалидация кэша
requestCache.invalidate(cacheKey); // По ключу
requestCache.invalidate(/^tasks-/); // По регулярному выражению

// Полная очистка кэша
requestCache.clear();
```

### Примеры в проекте

- `getWorkspaces()` - кэширование на 5 минут
- `getWorkspaceMembers()` - кэширование на 5 минут
- `getStatuses()` - кэширование на 5 минут
- `getCategories()` - кэширование на 5 минут
- `getTask()` - кэширование на 1 минуту

### Инвалидация кэша

Кэш автоматически инвалидируется при:
- Создании/обновлении/удалении ресурсов
- Logout пользователя
- Истечении TTL

## 2. Request Deduplicator (`src/utils/requestDeduplicator.ts`)

### Назначение

Предотвращает множественные одинаковые запросы, переиспользуя один Promise для всех одновременных вызовов.

### Использование

```typescript
import { requestDeduplicator } from '@/utils/requestDeduplicator';

export const getWorkspaces = async () => {
  // Сначала проверяем кэш
  const cached = requestCache.get('workspaces');
  if (cached) return cached;

  // Затем дедуплицируем запросы
  return requestDeduplicator.deduplicate('workspaces', async () => {
    const { data } = await $axios.get('workspaces');
    requestCache.set('workspaces', data, 300000);
    return data;
  });
};
```

### Как это работает

```typescript
// Несколько одновременных вызовов
getWorkspaces(); // Создает HTTP запрос
getWorkspaces(); // Переиспользует тот же Promise ⚡
getWorkspaces(); // Переиспользует тот же Promise ⚡

// = 1 реальный HTTP запрос вместо 3!
```

### Примеры в проекте

- `getWorkspaces()` - дедупликация запросов workspaces
- `getWorkspaceMembers()` - дедупликация по workspace ID
- `getStatuses()` - дедупликация запросов статусов
- `getCategories()` - дедупликация запросов категорий
- `getTask()` - дедупликация по task ID

## 3. Axios Configuration

### Обновления

- **Timeout**: 30 секунд
- **Retry Logic**: Автоматический retry для 500-level ошибок (2 попытки с экспоненциальной задержкой)
- **Token Refresh**: Автоматическое обновление токена в заголовках
- **Error Handling**: Централизованная обработка ошибок

## 4. Vuex State Normalization

### До

```javascript
state: {
  workspaces: [
    { id: 1, name: 'Workspace 1' },
    { id: 2, name: 'Workspace 2' }
  ]
}

// Доступ O(n)
const workspace = state.workspaces.find(w => w.id === 1);
```

### После

```javascript
state: {
  workspacesById: {
    1: { id: 1, name: 'Workspace 1' },
    2: { id: 2, name: 'Workspace 2' }
  }
}

// Доступ O(1)
const workspace = state.workspacesById[1];
```

### Getters

```javascript
// Массив workspace (для совместимости)
const workspaces = store.getters.workspaces;

// Прямой доступ по ID
const workspace = store.getters.getWorkspaceById(1);

// Текущий workspace
const current = store.getters.currentWorkspace;

// User setting по ключу
const setting = store.getters.userSettingByKey('current_workspace');
```

## 5. Memory Leak Fixes

### Event Listeners

Все event listeners теперь очищаются в `beforeUnmount`/`onUnmounted`:

```typescript
// ✅ Правильно
onMounted(() => {
  window.addEventListener('resize', handler);
});

onUnmounted(() => {
  window.removeEventListener('resize', handler);
});

// ❌ Неправильно (память утекает)
onMounted(() => {
  window.addEventListener('resize', handler);
});
// Нет cleanup!
```

### Timers

Все таймеры очищаются при unmount:

```typescript
// ✅ Правильно
const timeout = ref(null);

onMounted(() => {
  timeout.value = setTimeout(() => {}, 5000);
});

onUnmounted(() => {
  if (timeout.value) {
    clearTimeout(timeout.value);
  }
});
```

## 6. Component Optimizations

### Computed Properties vs Methods

```vue
<!-- ❌ Плохо - метод вызывается при каждом render -->
<template>
  <div>{{ getTaskTitle(task) }}</div>
</template>

<!-- ✅ Хорошо - computed кэшируется -->
<template>
  <div>{{ taskTitles[task.id] }}</div>
</template>

<script>
computed: {
  taskTitles() {
    return this.tasks.reduce((acc, task) => {
      acc[task.id] = this.getTaskTitle(task);
      return acc;
    }, {});
  }
}
</script>
```

## 7. Build Optimizations

### Vite Configuration

- **Manual Chunks**: Разделение vendor библиотек на отдельные chunks
- **Terser**: Удаление console.log в production
- **CSS Code Splitting**: Раздельная загрузка CSS
- **Chunk Size Warnings**: Предупреждения о больших chunks (>1MB)

### Chunk Strategy

- `vendor-vue`: Vue, Vue Router, Vuex
- `vendor-editor`: Editor.js и все плагины
- `vendor-ui`: UI библиотеки (Radix, Headless UI, Lucide)
- `vendor-utils`: Утилиты (axios, date-fns, GSAP)
- `vendor-markdown`: Markdown редактор
- `vendor-pusher`: Pusher и Laravel Echo

## 8. Development Logger

Вместо прямого использования `console.log`:

```typescript
import { devLog, devError, prodError } from '@/utils/devLogger';

// Логируется только в development
devLog('Debug info', data);

// Логируется только в development
devError('Dev error', error);

// Логируется всегда (для критических ошибок)
prodError('Critical error', error);
```

## Ожидаемые улучшения

- **Bundle Size**: Уменьшение на 40-60%
- **Initial Load**: Быстрее на 30-50%
- **Memory Usage**: Отсутствие утечек памяти
- **API Calls**: Снижение на 70-80% (благодаря кэшированию)
- **Rendering**: Быстрее на 20-30% (computed properties)
- **Navigation**: Мгновенная при использовании кэша

## Мониторинг

Для отслеживания производительности:

```javascript
// Размер кэша
console.log('Cache size:', requestCache.size());

// Проверка наличия в кэше
console.log('Has key:', requestCache.has('workspaces'));

// Список активных AbortControllers
// (доступно через Chrome DevTools)
```

## Best Practices

1. **Всегда используйте кэширование для GET запросов** к справочным данным (workspaces, statuses, categories)
2. **Инвалидируйте кэш** при мутирующих операциях (create, update, delete)
3. **Используйте AbortController** для отменяемых запросов (поиск, навигация)
4. **Очищайте event listeners** и timers в cleanup hooks
5. **Используйте computed properties** вместо методов в шаблонах
6. **Нормализуйте state** для O(1) доступа
7. **Избегайте deep watchers** когда возможно использовать shallow

## Troubleshooting

### Проблема: Данные не обновляются после изменения

**Решение**: Проверьте, что кэш инвалидируется после мутирующей операции:

```typescript
await updateTask(taskId, data);
requestCache.invalidate(`task-${taskId}`);
```

### Проблема: Множественные одинаковые запросы

**Решение**: Убедитесь, что используется кэширование:

```typescript
const data = await getWorkspaces(true); // useCache = true
```

### Проблема: Запросы не отменяются при навигации

**Решение**: Используйте AbortController:

```typescript
const controller = abortManager.create('unique-key');
try {
  await axios.get(url, { signal: controller.signal });
} finally {
  abortManager.delete('unique-key');
}
```
