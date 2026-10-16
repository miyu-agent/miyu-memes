//! 表情库（设计 34 第三节「一张表情包」）：工作目录里的 `library.db`。图本身是核心的 blob（`blob.put` 存的），这里只记它的
//! 哈希、尺寸、描述、用法、来源；编号是哈希的前 8 位十六进制，撞了往后多取。

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

/// 表结构：只加不改（以后要加格加新列）。
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS memes (
    id TEXT PRIMARY KEY,
    hash TEXT NOT NULL UNIQUE,
    media_type TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    description TEXT NOT NULL,
    usage TEXT NOT NULL,
    source TEXT NOT NULL,
    reason TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    uses INTEGER NOT NULL DEFAULT 0,
    last_used INTEGER,
    created INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS vectors (
    id TEXT NOT NULL,
    model TEXT NOT NULL,
    vector BLOB NOT NULL,
    PRIMARY KEY (id, model)
);
CREATE TABLE IF NOT EXISTS packs (
    package TEXT PRIMARY KEY,
    digest TEXT NOT NULL
);
";

/// 一张表情包。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Meme {
    pub id: String,
    /// 核心的 blob：`sha256:` 加 64 位十六进制。
    pub hash: String,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
    pub description: String,
    pub usage: String,
    /// 怎么来的：`{kind, package?, …}`（设计 34 第三节）。
    pub source: Value,
    pub reason: Option<String>,
    pub enabled: bool,
    pub uses: u64,
}

/// 存一张新的要交的。
#[derive(Debug, Clone)]
pub struct NewMeme {
    pub hash: String,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
    pub description: String,
    pub usage: String,
    pub source: Value,
    pub reason: Option<String>,
}

/// 打开的库。
pub struct Library {
    db: Connection,
}

/// 读库出的错：交回原话，工具照它写给她看。
pub type Failed = rusqlite::Error;

impl Library {
    /// 打开 `path`（没有的建上）。
    pub fn open(path: &Path) -> Result<Library, Failed> {
        let db = Connection::open(path)?;
        db.execute_batch(SCHEMA)?;
        Ok(Library { db })
    }

    /// 内存里的一份（测试用）。
    #[cfg(test)]
    pub fn in_memory() -> Result<Library, Failed> {
        let db = Connection::open_in_memory()?;
        db.execute_batch(SCHEMA)?;
        Ok(Library { db })
    }

    /// 存一张：同一张图已经在的交回已有的编号和 `false`，新存的交回新编号和 `true`。
    pub fn add(&self, meme: &NewMeme, now: i64) -> Result<(String, bool), Failed> {
        let existing: Option<String> = self
            .db
            .query_row(
                "SELECT id FROM memes WHERE hash = ?1",
                [&meme.hash],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok((id, false));
        }
        let id = self.short_id(&meme.hash)?;
        self.db.execute(
            "INSERT INTO memes (id, hash, media_type, width, height, description, usage, source, reason, created)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                meme.hash,
                meme.media_type,
                meme.width,
                meme.height,
                meme.description,
                meme.usage,
                meme.source.to_string(),
                meme.reason,
                now
            ],
        )?;
        Ok((id, true))
    }

    /// 哈希的前 8 位十六进制；撞了往后多取。
    fn short_id(&self, hash: &str) -> Result<String, Failed> {
        let hex = hash.strip_prefix("sha256:").unwrap_or(hash);
        for len in 8..=hex.len() {
            let id = &hex[..len];
            let taken: Option<i64> = self
                .db
                .query_row("SELECT 1 FROM memes WHERE id = ?1", [id], |row| row.get(0))
                .optional()?;
            if taken.is_none() {
                return Ok(id.to_string());
            }
        }
        Ok(hex.to_string())
    }

    /// 编号是 `id` 的那一张；停用的也交。
    pub fn get(&self, id: &str) -> Result<Option<Meme>, Failed> {
        self.db
            .query_row(&format!("{SELECT} WHERE id = ?1"), [id], meme_of)
            .optional()
    }

    /// 全部，照新到旧；`enabled_only` 的不带停用的。
    pub fn all(&self, enabled_only: bool) -> Result<Vec<Meme>, Failed> {
        let filter = if enabled_only {
            " WHERE enabled = 1"
        } else {
            ""
        };
        let mut statement = self
            .db
            .prepare(&format!("{SELECT}{filter} ORDER BY created DESC, id"))?;
        let rows = statement.query_map([], meme_of)?;
        rows.collect()
    }

    /// 改描述、用法：没写的不改。交回改没改到（没有这一张的是假）。
    pub fn edit(
        &self,
        id: &str,
        description: Option<&str>,
        usage: Option<&str>,
    ) -> Result<bool, Failed> {
        let changed = self.db.execute(
            "UPDATE memes SET description = COALESCE(?2, description), usage = COALESCE(?3, usage) WHERE id = ?1",
            params![id, description, usage],
        )?;
        if changed > 0 {
            self.db.execute("DELETE FROM vectors WHERE id = ?1", [id])?;
            // 内容包带来的被人改过：卸了包也留下（`packs.rs`）。
            self.db.execute(
                "UPDATE memes SET source = json_set(source, '$.edited', json('true'))
                 WHERE id = ?1 AND json_extract(source, '$.kind') = 'pack'",
                [id],
            )?;
        }
        Ok(changed > 0)
    }

    /// 停用、找回。
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<bool, Failed> {
        let changed = self.db.execute(
            "UPDATE memes SET enabled = ?2 WHERE id = ?1",
            params![id, enabled],
        )?;
        Ok(changed > 0)
    }

    /// 真删。
    pub fn purge(&self, id: &str) -> Result<bool, Failed> {
        self.db.execute("DELETE FROM vectors WHERE id = ?1", [id])?;
        let changed = self.db.execute("DELETE FROM memes WHERE id = ?1", [id])?;
        Ok(changed > 0)
    }

    /// 全部向量清掉（后台页「重新生成语义索引」）。
    pub fn clear_vectors(&self) -> Result<(), Failed> {
        self.db.execute("DELETE FROM vectors", [])?;
        Ok(())
    }

    /// 发了一次。
    pub fn used(&self, id: &str, now: i64) -> Result<(), Failed> {
        self.db.execute(
            "UPDATE memes SET uses = uses + 1, last_used = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        Ok(())
    }

    /// 记下 `id` 照模型 `model` 算的向量。
    pub fn put_vector(&self, id: &str, model: &str, vector: &[f32]) -> Result<(), Failed> {
        let bytes: Vec<u8> = vector.iter().flat_map(|x| x.to_le_bytes()).collect();
        self.db.execute(
            "INSERT OR REPLACE INTO vectors (id, model, vector) VALUES (?1, ?2, ?3)",
            params![id, model, bytes],
        )?;
        Ok(())
    }

    /// 照模型 `model` 算过向量的那几张：编号到向量。
    pub fn vectors(&self, model: &str) -> Result<Vec<(String, Vec<f32>)>, Failed> {
        let mut statement = self
            .db
            .prepare("SELECT id, vector FROM vectors WHERE model = ?1")?;
        let rows = statement.query_map([model], |row| {
            let bytes: Vec<u8> = row.get(1)?;
            let vector = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            Ok((row.get(0)?, vector))
        })?;
        rows.collect()
    }
}

const SELECT: &str = "SELECT id, hash, media_type, width, height, description, usage, source, reason, enabled, uses FROM memes";

fn meme_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<Meme> {
    let source: String = row.get(7)?;
    Ok(Meme {
        id: row.get(0)?,
        hash: row.get(1)?,
        media_type: row.get(2)?,
        width: row.get(3)?,
        height: row.get(4)?,
        description: row.get(5)?,
        usage: row.get(6)?,
        source: serde_json::from_str(&source).unwrap_or(Value::Null),
        reason: row.get(8)?,
        enabled: row.get(9)?,
        uses: row.get(10)?,
    })
}

mod packs;

#[cfg(test)]
mod tests;
