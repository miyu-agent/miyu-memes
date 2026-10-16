//! 找表情包（设计 34 第四节 `meme_search`）：按意思（核心的 `model.embed`，核心施工 X-17）加关键词。没配 embedding 的、算不出的
//! 退回只按关键词。向量照模型编号存：换了模型的旧向量不用，补算缺的那几张。

use crate::library::{Failed, Library, Meme};

/// 算向量的那一头：交回模型编号和照先后的向量，用不了的没有。
pub trait Embed {
    fn embed(&self, texts: &[String]) -> Option<(String, Vec<Vec<f32>>)>;
}

/// 问句的向量，和这个模型下每一张的编号、向量。
type Meaning = (Vec<f32>, Vec<(String, Vec<f32>)>);

/// 一次至多补算几张（`model.embed` 一批至多 64 条）。
const BATCH: usize = 64;

/// 拿去算向量的字：描述加用法。
fn text_of(meme: &Meme) -> String {
    format!("{} {}", meme.description, meme.usage)
}

/// 照 `query` 找，至多 `limit` 张，最像的在前；停用的不算。
pub fn search(
    library: &Library,
    embed: &dyn Embed,
    query: &str,
    limit: usize,
) -> Result<Vec<Meme>, Failed> {
    let memes = library.all(true)?;
    if memes.is_empty() {
        return Ok(Vec::new());
    }
    let meaning = meaning(library, embed, query, &memes)?;
    let mut scored: Vec<(f32, Meme)> = memes
        .into_iter()
        .map(|meme| {
            let close = meaning.as_ref().and_then(|(query, vectors)| {
                vectors
                    .iter()
                    .find(|(id, _)| *id == meme.id)
                    .map(|(_, vector)| dot(query, vector))
            });
            let words = keyword(query, &text_of(&meme));
            (close.unwrap_or(0.0) + words, meme)
        })
        .filter(|(score, _)| *score > 0.0 || meaning.is_some())
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    Ok(scored
        .into_iter()
        .take(limit)
        .map(|(_, meme)| meme)
        .collect())
}

/// 问句的向量和这个模型下每一张的向量；用不了的没有。缺向量的先补。
fn meaning(
    library: &Library,
    embed: &dyn Embed,
    query: &str,
    memes: &[Meme],
) -> Result<Option<Meaning>, Failed> {
    let Some((model, mut asked)) = embed.embed(&[query.to_string()]) else {
        return Ok(None);
    };
    let Some(query) = asked.pop() else {
        return Ok(None);
    };
    let have = library.vectors(&model)?;
    let missing: Vec<&Meme> = memes
        .iter()
        .filter(|meme| !have.iter().any(|(id, _)| *id == meme.id))
        .collect();
    for chunk in missing.chunks(BATCH) {
        let texts: Vec<String> = chunk.iter().map(|meme| text_of(meme)).collect();
        let Some((same, vectors)) = embed.embed(&texts) else {
            break;
        };
        if same != model {
            break;
        }
        for (meme, vector) in chunk.iter().zip(vectors) {
            library.put_vector(&meme.id, &model, &vector)?;
        }
    }
    Ok(Some((query, library.vectors(&model)?)))
}

/// 重新生成语义索引：清掉全部向量，照现在的模型给每一张（停用的也算）重算。交回模型和算了几张；用不了的没有。
pub fn reindex(library: &Library, embed: &dyn Embed) -> Result<Option<(String, usize)>, Failed> {
    let memes = library.all(false)?;
    library.clear_vectors()?;
    if memes.is_empty() {
        return Ok(Some((String::new(), 0)));
    }
    let mut model: Option<String> = None;
    let mut done = 0;
    for chunk in memes.chunks(BATCH) {
        let texts: Vec<String> = chunk.iter().map(text_of).collect();
        let Some((same, vectors)) = embed.embed(&texts) else {
            return Ok(model.map(|model| (model, done)));
        };
        if model.as_ref().is_some_and(|first| *first != same) {
            break;
        }
        for (meme, vector) in chunk.iter().zip(vectors) {
            library.put_vector(&meme.id, &same, &vector)?;
            done += 1;
        }
        model = Some(same);
    }
    Ok(model.map(|model| (model, done)))
}

/// 归一化过的向量的点积就是余弦。
fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// 关键词的分：问句里每两个相邻的字（汉字一个字也算）在文本里出现一次加一点，照问句长短折算到 0 到 0.5。
fn keyword(query: &str, text: &str) -> f32 {
    let text = text.to_lowercase();
    let chars: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if chars.is_empty() {
        return 0.0;
    }
    let grams: Vec<String> = if chars.len() == 1 {
        vec![chars[0].to_string()]
    } else {
        chars.windows(2).map(|pair| pair.iter().collect()).collect()
    };
    let hits = grams
        .iter()
        .filter(|gram| text.contains(gram.as_str()))
        .count();
    #[expect(clippy::cast_precision_loss, reason = "几十个字以内，精度够")]
    let share = hits as f32 / grams.len() as f32;
    share / 2.0
}

#[cfg(test)]
mod tests;
