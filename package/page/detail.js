// 详情：大图、描述、用法（能改）、来源、收藏理由、使用次数、尺寸；保存、停用或启用、删除。宽的时候在网格右边，窄的盖在
// 网格上面（style.css）。

export function detailView(ui) {
  const { h, say } = ui;
  const img = h('img', { class: 'large', alt: '' });
  const description = h('textarea', { rows: 4, 'aria-label': say('description') });
  const usage = h('textarea', { rows: 3, 'aria-label': say('usage') });
  const facts = h('dl', { class: 'facts' });
  const note = h('p', { class: 'note' });
  const save = h('button', { class: 'button primary', type: 'button', text: say('save'), disabled: true });
  const toggle = h('button', { class: 'button', type: 'button' });
  const purge = h('button', { class: 'button danger', type: 'button', text: say('purge') });
  const close = h('button', { class: 'icon-button', type: 'button', 'aria-label': say('close'), text: '×', onclick: () => ui.select(null) });
  const el = h('aside', { class: 'detail', hidden: true },
    h('div', { class: 'detail-head' }, close),
    h('div', { class: 'frame' }, img),
    h('label', { class: 'field' }, h('span', { text: say('description') }), description),
    h('label', { class: 'field' }, h('span', { text: say('usage') }), usage),
    facts,
    h('div', { class: 'actions' }, purge, h('span', { class: 'spacer' }), toggle, save),
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

  function sourceText(source) {
    const kind = source?.kind;
    return ['ai', 'human', 'package'].includes(kind) ? say(`source/${kind}`) : String(kind ?? '');
  }

  function fact(name, value) {
    return value === '' || value == null ? [] : [h('dt', { text: say(name) }), h('dd', { text: String(value) })];
  }

  return {
    el,
    /** 换成 `next` 这一张（null 是收起）；同一张只换会变的几格，人正在改的字不动。 */
    show(next) {
      const same = meme && next && meme.id === next.id;
      meme = next;
      el.hidden = !next;
      if (!next) return;
      if (!same) {
        img.removeAttribute('src');
        ui.images.url(next.id).then((src) => { if (meme?.id === next.id) img.src = src; }, () => {});
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
