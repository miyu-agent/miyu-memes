// 详情：大图、描述、用法（能改）、来源、收藏理由、使用次数、尺寸；保存、停用或启用、删除。宽的时候在网格右边，窄的盖在
// 网格上面（style.css）。

/** 把 AI 写的字放进框里，亮一下（`style.css` 的 `.is-filled`）。 */
export function fill(area, text) {
  area.value = text ?? '';
  area.classList.remove('is-filled');
  void area.offsetWidth;
  area.classList.add('is-filled');
}

export function detailView(ui) {
  const { h, say } = ui;
  const img = h('img', { class: 'large', alt: '' });
  img.addEventListener('load', () => img.classList.add('is-loaded'));
  const description = h('textarea', { rows: 4, 'aria-label': say('description') });
  const usage = h('textarea', { rows: 3, 'aria-label': say('usage') });
  const facts = h('dl', { class: 'facts' });
  const note = h('p', { class: 'note' });
  const save = h('button', { class: 'button primary', type: 'button', text: say('save'), disabled: true });
  const toggle = h('button', { class: 'button', type: 'button' });
  const purge = h('button', { class: 'button danger', type: 'button', text: say('purge') });
  const redescribe = h('button', { class: 'button', type: 'button', text: say('describe') });
  const close = h('button', { class: 'icon-button', type: 'button', 'aria-label': say('close'), text: '×', onclick: () => ui.select(null) });
  const el = h('aside', { class: 'detail', hidden: true },
    h('div', { class: 'detail-head' }, close),
    h('div', { class: 'frame' }, img),
    h('label', { class: 'field' }, h('span', { text: say('description') }), description),
    h('label', { class: 'field' }, h('span', { text: say('usage') }), usage),
    facts,
    h('div', { class: 'actions' }, purge, h('span', { class: 'spacer' }), redescribe, toggle, save),
    note);
  let meme = null;

  const changed = () => meme
    && (description.value.trim() !== meme.description || usage.value.trim() !== meme.usage)
    && description.value.trim() && usage.value.trim();
  description.addEventListener('input', () => { save.disabled = !changed(); note.textContent = ''; });
  usage.addEventListener('input', () => { save.disabled = !changed(); note.textContent = ''; });

  /** 办一件事：按钮先灰掉，错了写在底下。 */
  async function act(button, work) {
    button.disabled = true;
    note.classList.remove('error');
    try {
      await work();
    } catch (error) {
      note.classList.add('error');
      note.textContent = ui.reasonOf(error);
    } finally {
      button.disabled = button === save ? !changed() : false;
    }
  }

  save.addEventListener('click', () => act(save, async () => {
    await ui.call('update', { id: meme.id, description: description.value.trim(), usage: usage.value.trim() });
    await ui.reload();
    note.textContent = say('saved');
  }));
  toggle.addEventListener('click', () => act(toggle, async () => {
    await ui.call(meme.enabled ? 'delete' : 'restore', { id: meme.id });
    await ui.reload();
  }));
  purge.addEventListener('click', () => act(purge, async () => {
    if (!(await ui.confirmed(say('purge_confirm'), say('purge')))) return;
    const id = meme.id;
    await ui.call('purge', { id });
    await ui.purged(id);
  }));

  // 重新描述（项目主人 10-16）：看图模型写好填进两个框，亮一下，人看了点保存才存。
  redescribe.addEventListener('click', async () => {
    const id = meme.id;
    redescribe.disabled = true;
    redescribe.textContent = say('describing');
    note.classList.remove('error');
    note.textContent = '';
    try {
      const got = await ui.call('describe', { id, language: ui.language() });
      if (meme?.id !== id) return;
      fill(description, got.description);
      fill(usage, got.usage);
      save.disabled = !changed();
      note.textContent = say('described');
    } catch (error) {
      note.classList.add('error');
      note.textContent = ui.describeError(error);
    } finally {
      redescribe.disabled = false;
      redescribe.textContent = say('describe');
    }
  });

  function sourceText(source) {
    const kind = source?.kind;
    const named = ['ai', 'human', 'pack', 'package'].includes(kind) ? say(`source/${kind}`) : String(kind ?? '');
    return [named, ...whence(source)].join(' · ');
  }

  /** 聊天里存的（施工 M-9）多几样：哪个群、私聊，谁发的，什么时候。 */
  function whence(source) {
    const parts = [];
    const [, place, id] = String(source?.venue ?? '').split(':');
    if (place === 'group' || place === 'private') parts.push(say(`venue/${place}`, { id }));
    const sender = source?.sender?.name || source?.sender?.id;
    if (sender) parts.push(sender);
    if (Number.isFinite(source?.at)) parts.push(when(source.at));
    return parts;
  }

  /** 毫秒时刻写成本地的 `2026-10-26 16:22`。 */
  function when(at) {
    const d = new Date(at);
    const two = (n) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}`;
  }

  function fact(name, value) {
    return value === '' || value == null ? [] : [h('dt', { text: say(name) }), h('dd', { text: String(value) })];
  }

  return {
    el,
    /** 换成 `next` 这一张（null 是收起）；同一张只换会变的几格，人正在改的字不动。 */
    show(next) {
      const same = meme && next && meme.id === next.id;
      const opening = !meme && next;
      meme = next;
      el.hidden = !next;
      if (!next) return;
      if (opening) {
        el.classList.remove('is-opening');
        void el.offsetWidth;
        el.classList.add('is-opening');
      }
      if (!same) {
        // 旧图留到新图到了再换，框高固定（style.css .frame）：换一张不塌、不跳。
        if (opening) img.classList.remove('is-loaded');
        ui.images.url(next.id).then((src) => { if (meme?.id === next.id) img.src = src; }, () => {
          if (meme?.id === next.id) img.removeAttribute('src');
        });
        description.value = next.description;
        usage.value = next.usage;
        note.textContent = '';
      }
      img.alt = next.description;
      facts.replaceChildren(
        ...fact('source', sourceText(next.source)),
        ...fact('reason', next.reason ?? ''),
        ...fact('uses', next.uses),
        ...fact('size', `${next.width} × ${next.height}`));
      toggle.textContent = say(next.enabled ? 'disable' : 'enable');
      save.disabled = !changed();
    },
  };
}
