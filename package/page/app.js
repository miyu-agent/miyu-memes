// 表情包的后台页（设计 34 第七节；核心 docs/blueprint/package-pages.md「框和网页之间怎么说」）：通道、字、颜色，页顶一排
// （搜索、来源、显示已停用、上传），下面网格和详情。网格在 grid.js，详情在 detail.js，传图在 upload.js，看图在 images.js。
//
// - 页面跑在网页给的隔离框里：只经网页交来的 MessageChannel 照 JSON-RPC 说，方法 context、settings.get、call（转
//   package.call → 包的 method.call）；推送 theme.changed、settings.changed。
// - 不轮询：改了什么当场重读；框重新看得见时（visibilitychange）重读一次，接住她这期间存的。
import { TEXTS } from './texts.js';
import { imageLoader } from './images.js';
import { gridView } from './grid.js';
import { detailView } from './detail.js';
import { openUpload } from './upload.js';

(() => {
  const COLORS = {
    accent: '--accent', surface: '--surface', surface_2: '--surface-2', text: '--text', text_soft: '--text-soft',
    line: '--line', danger: '--danger',
  };
  const SEARCH_WAIT_MS = 250;
  const SEARCH_LIMIT = 50;

  const app = /** @type {HTMLElement} */ (document.getElementById('app'));
  const state = {
    said: TEXTS.en, channel: null, values: {}, memes: [], query: '', source: 'all', showDisabled: false,
    selected: null, timer: 0,
  };

  function pick(language) {
    const head = String(language ?? '').toLowerCase().split(/[-_]/)[0];
    return TEXTS[head] ?? TEXTS.en;
  }

  function say(key, fields = {}) {
    const template = state.said[key] ?? TEXTS.en[key];
    if (template == null) return key;
    return template.replace(/\{([a-z_]+)\}/g, (match, name) => (name in fields ? String(fields[name]) : match));
  }

  /** 造一个元素：`text` 是字，`on…` 是事件，别的是属性；孩子是元素或字（字一律当字）。 */
  function h(tag, props = {}, ...children) {
    const el = document.createElement(tag);
    for (const [key, value] of Object.entries(props)) {
      if (value == null || value === false) continue;
      if (key === 'class') el.className = value;
      else if (key === 'text') el.textContent = value;
      else if (key.startsWith('on')) el.addEventListener(key.slice(2), value);
      else el.setAttribute(key, value === true ? '' : String(value));
    }
    el.append(...children.flat().filter((child) => child != null && child !== false));
    return el;
  }

  function reasonOf(error) {
    return error?.message ?? String(error);
  }

  /** 页面里的对话框问一句：按 `yes` 交回真。 */
  function confirmed(question, yes) {
    return new Promise((resolve) => {
      const button = (text, value, kind) => h('button', {
        class: `button ${kind}`, type: 'button', text, onclick: () => dialog.close(value),
      });
      const dialog = h('dialog', { class: 'sheet confirm' },
        h('p', { text: question }),
        h('div', { class: 'actions' }, button(say('cancel'), 'no', ''), button(yes, 'yes', 'danger')));
      dialog.addEventListener('close', () => { dialog.remove(); resolve(dialog.returnValue === 'yes'); });
      document.body.append(dialog);
      dialog.showModal();
    });
  }

  class Channel {
    constructor(port) {
      this.port = port;
      this.n = 0;
      this.waiting = new Map();
      this.handlers = {};
      port.onmessage = (event) => this.receive(event.data);
    }

    receive(message) {
      if (!message || typeof message !== 'object') return;
      if (message.id == null) {
        if (typeof message.method === 'string') this.handlers[message.method]?.(message.params ?? {});
        return;
      }
      const waiter = this.waiting.get(message.id);
      if (!waiter) return;
      this.waiting.delete(message.id);
      if (!message.error) return waiter.resolve(message.result);
      const { message: said, code, data } = message.error;
      waiter.reject(Object.assign(new Error(said ?? String(code)), { code, data: data ?? null }));
    }

    call(method, params) {
      return new Promise((resolve, reject) => {
        const id = ++this.n;
        this.waiting.set(id, { resolve, reject });
        this.port.postMessage(params === undefined ? { jsonrpc: '2.0', id, method } : { jsonrpc: '2.0', id, method, params });
      });
    }

    on(method, handler) { this.handlers[method] = handler; }
  }

  function look(theme, colors) {
    const root = document.documentElement;
    root.dataset.theme = theme === 'dark' ? 'dark' : 'light';
    for (const [name, variable] of Object.entries(COLORS)) {
      const color = colors?.[name];
      if (typeof color === 'string' && color) root.style.setProperty(variable, color);
      else root.style.removeProperty(variable);
    }
  }

  function centered(text, again) {
    app.className = 'boot';
    app.replaceChildren(h('p', { class: again ? 'error' : 'boot-wait', text }),
      again && h('button', { class: 'button primary', type: 'button', text: say('retry'), onclick: again }));
  }

  /** 调包的方法 `method`（经网页的 call 转 package.call）。 */
  const call = (method, params) => state.channel.call('call', params === undefined ? { method } : { method, params });

  const ui = {
    h, say, call, confirmed, reasonOf,
    images: null,
    maxMb: () => Number(state.values['miyu-memes.max_size_mb'] ?? 5),
    /** 选中 `id` 那一张（null 是关掉详情）。 */
    select: (id) => { state.selected = id; paint(); },
    /** 改了什么之后：重读一遍。 */
    reload: () => load(),
    /** 真删了的：不再选着、不再记着它的图。 */
    purged: (id) => { ui.images.forget(id); if (state.selected === id) state.selected = null; return load(); },
  };

  let grid;
  let detail;
  let view;

  /** 照筛选算这一屏的几张。 */
  function shown() {
    return state.memes.filter((meme) => (state.showDisabled || meme.enabled)
      && (state.source === 'all' || meme.source?.kind === state.source));
  }

  function paint() {
    const memes = shown();
    const nothing = !state.query.trim() && !state.memes.length;
    grid.paint(memes, state.selected, memes.length ? '' : say(nothing ? 'empty' : 'no_match'));
    const meme = state.memes.find((one) => one.id === state.selected) ?? null;
    view.classList.toggle('has-detail', Boolean(meme));
    detail.show(meme);
  }

  /** 读一遍：有搜索词的照她的搜法（只搜启用的），没有的列全部。 */
  async function load() {
    const query = state.query.trim();
    const got = query ? await call('search', { query, limit: SEARCH_LIMIT }) : await call('list');
    state.memes = got?.memes ?? [];
    paint();
  }

  function toolbar() {
    const search = h('input', {
      class: 'search', type: 'search', placeholder: say('search'), 'aria-label': say('search'),
      oninput: () => {
        clearTimeout(state.timer);
        state.timer = setTimeout(() => { state.query = search.value; load().catch(() => {}); }, SEARCH_WAIT_MS);
      },
    });
    const option = (value) => h('option', { value, text: say(`source/${value}`) });
    const source = h('select', {
      class: 'select', 'aria-label': say('source'),
      onchange: () => { state.source = source.value; paint(); },
    }, ['all', 'ai', 'human', 'package'].map(option));
    const disabled = h('input', {
      type: 'checkbox', onchange: () => { state.showDisabled = disabled.checked; paint(); },
    });
    return h('header', { class: 'bar' }, search, source,
      h('label', { class: 'check' }, disabled, h('span', { text: say('show_disabled') })),
      h('button', { class: 'button primary', type: 'button', text: say('upload'), onclick: () => openUpload(ui) }));
  }

  function shell() {
    ui.images = imageLoader(call);
    grid = gridView(ui);
    detail = detailView(ui);
    view = h('div', { class: 'view' }, grid.el, detail.el);
    app.className = 'shell';
    app.replaceChildren(toolbar(), view);
  }

  async function start() {
    centered(say('loading'));
    try {
      const context = await state.channel.call('context');
      state.said = pick(context?.language);
      document.documentElement.lang = context?.language ?? '';
      look(context?.theme, context?.colors);
      state.values = (await state.channel.call('settings.get'))?.values ?? {};
      shell();
      await load();
    } catch (error) {
      centered(say('failed', { reason: reasonOf(error) }), start);
    }
  }

  function listen(channel) {
    channel.on('theme.changed', (params) => look(params.theme, params.colors));
    channel.on('settings.changed', async () => {
      try { state.values = (await channel.call('settings.get'))?.values ?? {}; } catch { /* 照手里的 */ }
    });
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible' && grid) load().catch(() => {});
    });
  }

  window.addEventListener('message', (event) => {
    if (state.channel || event.source !== window.parent || event.data?.miyu !== 'port' || !event.ports?.[0]) return;
    state.channel = new Channel(event.ports[0]);
    listen(state.channel);
    start();
  });
  centered(say('loading'));
})();
