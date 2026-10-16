// 网格：一张一格，缩略图加描述的头一行；停用的变淡、角上写「已停用」。点一格交给 ui.select。格子照编号留着，重画时只换
// 字和选中，图不重取。

/** 描述的头一行（句号、换行为止）。 */
function firstLine(text) {
  const line = String(text ?? '').split(/\n/)[0];
  const stop = line.search(/[。.!！?？]/);
  return stop > 0 ? line.slice(0, stop + 1) : line;
}

export function gridView(ui) {
  const { h, say } = ui;
  const list = h('div', { class: 'cells', role: 'list' });
  const note = h('p', { class: 'empty' });
  const el = h('section', { class: 'grid' }, list, note);
  const cells = new Map();

  function cell(meme) {
    const img = h('img', { alt: '', draggable: 'false' });
    img.addEventListener('load', () => img.classList.add('is-loaded'));
    ui.images.watch(img, meme.id);
    const caption = h('span', { class: 'caption' });
    const badge = h('span', { class: 'badge', text: say('disabled') });
    const button = h('button', { class: 'cell enter', type: 'button', role: 'listitem', onclick: () => ui.select(meme.id) },
      h('span', { class: 'thumb' }, img, badge), caption);
    return { button, caption, badge };
  }

  return {
    el,
    /** 画这几张；`selected` 是选着的编号，`message` 是一张都没有时那一句。 */
    paint(memes, selected, message) {
      const keep = new Set(memes.map((meme) => meme.id));
      for (const id of cells.keys()) if (!keep.has(id)) cells.delete(id);
      const order = memes.map((meme) => {
        const known = cells.get(meme.id) ?? cell(meme);
        cells.set(meme.id, known);
        known.caption.textContent = firstLine(meme.description);
        known.button.title = meme.description;
        known.button.classList.toggle('is-on', meme.id === selected);
        known.button.classList.toggle('is-off', !meme.enabled);
        known.badge.hidden = meme.enabled;
        return known.button;
      });
      list.replaceChildren(...order);
      note.textContent = message;
    },
  };
}
