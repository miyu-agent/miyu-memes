// 传图前先在页面里缩（项目主人 10-16：超了缩，不拒）：办法同包里的 shrink.rs，长边每次缩到四分之三，有透明的编成 PNG、
// 没有的编成 JPEG（0.85）。核心一条消息最多 1 MiB，整张 base64 要装进一条，所以页面这边的上限取「单张上限」和 WIRE 小的那个。
// 动图缩了就不动了：装不下的交回 null，由调的一方说太大。

/** 一条消息里图最多多少字节：1 MiB 的行，base64 涨三分之一，再留出 JSON 其余几格的地方。 */
export const WIRE = 700 * 1024;

/** 长边最少缩到多少像素。 */
const SMALLEST = 64;

/** 有没有不透明以外的点：缩到最多 256 像素看，够认出透明底。 */
function transparent(bitmap) {
  const scale = Math.min(1, 256 / Math.max(bitmap.width, bitmap.height));
  const canvas = new OffscreenCanvas(Math.max(1, Math.round(bitmap.width * scale)), Math.max(1, Math.round(bitmap.height * scale)));
  const context = canvas.getContext('2d');
  context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
  for (let at = 3; at < data.length; at += 4) if (data[at] < 255) return true;
  return false;
}

function renamed(name, ext) {
  const dot = name.lastIndexOf('.');
  return `${dot > 0 ? name.slice(0, dot) : name}.${ext}`;
}

/** `file` 装得进 `limit` 字节的原样交回；装不进的缩好交回新的 File；缩不进的（动图、缩到最小还大）交回 null。 */
export async function fitted(file, limit) {
  if (file.size <= limit) return file;
  if (file.type === 'image/gif') return null;
  const bitmap = await createImageBitmap(file);
  const alpha = file.type !== 'image/jpeg' && transparent(bitmap);
  const longest = Math.max(bitmap.width, bitmap.height);
  let side = longest;
  for (;;) {
    const scale = side / longest;
    const canvas = new OffscreenCanvas(Math.max(1, Math.round(bitmap.width * scale)), Math.max(1, Math.round(bitmap.height * scale)));
    canvas.getContext('2d').drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    const blob = await canvas.convertToBlob(alpha ? { type: 'image/png' } : { type: 'image/jpeg', quality: 0.85 });
    if (blob.size <= limit) {
      bitmap.close();
      return new File([blob], renamed(file.name, alpha ? 'png' : 'jpg'), { type: blob.type });
    }
    if (side <= SMALLEST) {
      bitmap.close();
      return null;
    }
    side = Math.max(SMALLEST, Math.floor((side * 3) / 4));
  }
}
