// Retire the legacy URL-only API cache, which was not isolated by account.
self.addEventListener('activate', event => {
 event.waitUntil(caches.delete('api-cache'));
});
