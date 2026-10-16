// 传图：对话框里拖进或选一张，人写描述和用法，读成 base64 交给包的 `upload`（包再 blob.put 存进核心）。超过设置的大小、不是
// 图的当场说；库里已经有的照样选中它，说一句「已在库中」。

import { fill } from './detail.js';

/** 读成 base64（不带 data: 那一截）。 */
function base64(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).replace(/^data:[^,]*,/, ''));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

export function openUpload(ui) {
  const { h, say } = ui;
  let file = null;
  let preview = '';
  const picker = h('input', { type: 'file', accept: 'image/*', class: 'offscreen' });
  const shown = h('img', { class: 'preview', alt: '', hidden: true });
  const hint = h('span', { text: say('drop') });
  const zone = h('label', { class: 'drop' }, picker, shown, hint);
  const description = h('textarea', { rows: 3 });
  const usage = h('textarea', { rows: 2 });
  const note = h('p', { class: 'note error' });
  const submit = h('button', { class: 'button primary', type: 'button', text: say('upload'), disabled: true });
  const write = h('button', { class: 'button', type: 'button', text: say('ai_write'), disabled: true });
  const cancel = h('button', { class: 'button', type: 'button', text: say('cancel') });
  const dialog = h('dialog', { class: 'sheet upload' },
    h('h2', { text: say('upload_title') }),
    zone,
    h('label', { class: 'field' }, h('span', { text: say('description') }), description),
    h('label', { class: 'field' }, h('span', { text: say('usage') }), usage),
    note,
    h('div', { class: 'actions' }, write, h('span', { class: 'spacer' }), cancel, submit));

  const ready = () => Boolean(file && description.value.trim() && usage.value.trim());
  const refresh = () => { submit.disabled = !ready(); };

  function choose(next) {
    note.textContent = '';
    if (!next) return;
    if (!next.type.startsWith('image/')) { note.textContent = say('not_image'); return; }
    if (next.size > ui.maxMb() * 1024 * 1024) { note.textContent = say('too_big', { max: ui.maxMb() }); return; }
    file = next;
    if (preview) URL.revokeObjectURL(preview);
    preview = URL.createObjectURL(next);
    shown.src = preview;
    shown.hidden = false;
    hint.hidden = true;
    write.disabled = false;
    refresh();
  }

  picker.addEventListener('change', () => choose(picker.files?.[0]));
  zone.addEventListener('dragover', (event) => { event.preventDefault(); zone.classList.add('is-over'); });
  zone.addEventListener('dragleave', () => zone.classList.remove('is-over'));
  zone.addEventListener('drop', (event) => {
    event.preventDefault();
    zone.classList.remove('is-over');
    choose(event.dataTransfer?.files?.[0]);
  });
  description.addEventListener('input', refresh);
  usage.addEventListener('input', refresh);
  cancel.addEventListener('click', () => dialog.close());
  dialog.addEventListener('close', () => { if (preview) URL.revokeObjectURL(preview); dialog.remove(); });

  // 让 AI 写（项目主人 10-16）：图先交给包、包存进核心再让看图模型写，填进两个框；人改了再上传。
  write.addEventListener('click', async () => {
    write.disabled = true;
    write.textContent = say('describing');
    note.textContent = '';
    try {
      const got = await ui.call('describe', { data: await base64(file), name: file.name, language: ui.language() });
      fill(description, got.description);
      fill(usage, got.usage);
      refresh();
    } catch (error) {
      note.textContent = ui.describeError(error);
    } finally {
      write.disabled = false;
      write.textContent = say('ai_write');
    }
  });

  submit.addEventListener('click', async () => {
    if (!ready()) { note.textContent = say('required'); return; }
    submit.disabled = true;
    note.textContent = '';
    try {
      const got = await ui.call('upload', {
        data: await base64(file), name: file.name, description: description.value.trim(), usage: usage.value.trim(),
      });
      await ui.reload();
      ui.select(got.id);
      if (!got.created) {
        note.textContent = say('exists');
        setTimeout(() => dialog.close(), 1200);
      } else {
        dialog.close();
      }
    } catch (error) {
      const reason = ui.reasonOf(error);
      note.textContent = reason === 'too_big' ? say('too_big', { max: ui.maxMb() })
        : reason === 'not_image' ? say('not_image') : ui.reasonOf(error);
      refresh();
    }
  });

  document.body.append(dialog);
  dialog.showModal();
}
