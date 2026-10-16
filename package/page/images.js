// 看图：框里拿不到核心的 blob，经包的 `image {id}` 方法取整张的 base64，转成 blob: 地址（框的 CSP 给 blob:）。取过的记着；
// 同时最多取 PARALLEL 张；格子进了视口才取（IntersectionObserver），不在视口的不取。

const PARALLEL = 4;

/** `call(method, params)` 是调包方法的那一个。交回 `{url(id), watch(img, id), forget(id)}`。 */
export function imageLoader(call) {
  const urls = new Map();
  const pending = new Map();
  const queue = [];
  let running = 0;

  function pump() {
    while (running < PARALLEL && queue.length) {
      const { id, resolve, reject } = queue.shift();
      running += 1;
      call('image', { id })
        .then(({ media_type: type, data }) => {
          const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
          const url = URL.createObjectURL(new Blob([bytes], { type }));
          urls.set(id, url);
          resolve(url);
        }, reject)
        .finally(() => {
          pending.delete(id);
          running -= 1;
          pump();
        });
    }
  }

  /** 这一张的地址：取过的当场给，没取的排队取。 */
  function url(id) {
    if (urls.has(id)) return Promise.resolve(urls.get(id));
    if (!pending.has(id)) {
      pending.set(id, new Promise((resolve, reject) => queue.push({ id, resolve, reject })));
      pump();
    }
    return pending.get(id);
  }

  const seen = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      const img = /** @type {HTMLImageElement} */ (entry.target);
      seen.unobserve(img);
      url(img.dataset.id).then((src) => { img.src = src; }, () => img.classList.add('is-broken'));
    }
  }, { rootMargin: '200px' });

  return {
    url,
    /** `img` 进了视口再取 `id` 那一张放上。 */
    watch(img, id) {
      img.dataset.id = id;
      if (urls.has(id)) img.src = urls.get(id);
      else seen.observe(img);
    },
    /** 真删了的那一张不再记着。 */
    forget(id) {
      const known = urls.get(id);
      if (known) URL.revokeObjectURL(known);
      urls.delete(id);
    },
  };
}
