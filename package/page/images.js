// 看图：框里拿不到核心的 blob，经包的方法取 base64，转成 blob: 地址（框的 CSP 给 blob:）。取过的记着；同时最多取 PARALLEL
// 张（小图、原图合着算）；格子进了视口才取（IntersectionObserver），不在视口的不取。
//
// - 列表用小图 `thumb {id}`：长边 256 以内，一条回应就是一张。
// - 详情用原图 `image {id, offset}`：一次回一块，带 `next` 的照它接着取，各块解开接起来就是整张（块的边界是 3 的倍数，
//   每块的 base64 各自解得开）。核心收一行最多 1 MiB，整张放一条回应的话大图会被拒（施工 M-14）。
// - 取不到的交回拒绝，谁要的谁显示「加载失败」，不拦别的图；失败的不记，下次要时重取。

const PARALLEL = 4;

/** base64 解成字节。 */
function bytesOf(data) {
  return Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
}

/** `call(method, params)` 是调包方法的那一个。交回 `{url(id), watch(img, id, failed), forget(id)}`。 */
export function imageLoader(call) {
  /** 小图、原图各记各的：编号 → 地址，编号 → 正在取的。 */
  const kinds = { thumb: { urls: new Map(), pending: new Map() }, full: { urls: new Map(), pending: new Map() } };
  const queue = [];
  let running = 0;

  /** 小图：一条回应。 */
  async function thumb(id) {
    const { media_type: type, data } = await call('thumb', { id });
    return new Blob([bytesOf(data)], { type });
  }

  /** 原图：照 `next` 一块一块取到头。核心不往前走的当失败，不死循环。 */
  async function full(id) {
    const parts = [];
    let type = '';
    let offset = 0;
    for (;;) {
      const got = await call('image', { id, offset });
      type = got.media_type;
      parts.push(bytesOf(got.data));
      if (got.next == null) break;
      if (!(got.next > offset)) throw new Error('image stuck');
      offset = got.next;
    }
    return new Blob(parts, { type });
  }

  const fetchers = { thumb, full };

  function pump() {
    while (running < PARALLEL && queue.length) {
      const { kind, id, resolve, reject } = queue.shift();
      running += 1;
      fetchers[kind](id)
        .then((blob) => {
          const url = URL.createObjectURL(blob);
          kinds[kind].urls.set(id, url);
          resolve(url);
        }, reject)
        .finally(() => {
          kinds[kind].pending.delete(id);
          running -= 1;
          pump();
        });
    }
  }

  /** 这一张的地址：取过的当场给，没取的排队取。 */
  function get(kind, id) {
    const { urls, pending } = kinds[kind];
    if (urls.has(id)) return Promise.resolve(urls.get(id));
    if (!pending.has(id)) {
      pending.set(id, new Promise((resolve, reject) => queue.push({ kind, id, resolve, reject })));
      pump();
    }
    return pending.get(id);
  }

  const failures = new WeakMap();
  const seen = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      const img = /** @type {HTMLImageElement} */ (entry.target);
      seen.unobserve(img);
      get('thumb', img.dataset.id).then((src) => { img.src = src; }, () => {
        img.classList.add('is-broken');
        failures.get(img)?.();
      });
    }
  }, { rootMargin: '200px' });

  return {
    /** `id` 那一张原图的地址（详情用）。 */
    url: (id) => get('full', id),
    /** `img` 进了视口再取 `id` 那一张的小图放上；取不到时叫 `failed`。 */
    watch(img, id, failed) {
      img.dataset.id = id;
      if (failed) failures.set(img, failed);
      const known = kinds.thumb.urls.get(id);
      if (known) img.src = known;
      else seen.observe(img);
    },
    /** 真删了的那一张不再记着。 */
    forget(id) {
      for (const { urls } of Object.values(kinds)) {
        const known = urls.get(id);
        if (known) URL.revokeObjectURL(known);
        urls.delete(id);
      }
    },
  };
}
