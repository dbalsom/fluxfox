var cacheName = 'egui-template-pwa';
// var filesToCache = [
//   './',
//   './index.html',
//   './ffweb.js',
//   './ffweb_bg.wasm',
// ];

/*var filesToCache = [];

/!* Start the service worker and cache all the app's content *!/
self.addEventListener('install', function (e) {
  e.waitUntil(
    caches.open(cacheName).then(function (cache) {
      return cache.addAll(filesToCache);
    })
  );
});

/!* Serve cached content when offline *!/
self.addEventListener('fetch', function (e) {
  e.respondWith(
    caches.match(e.request).then(function (response) {
      return response || fetch(e.request);
    })
  );
});*/
