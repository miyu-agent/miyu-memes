//! 内容包带来的那些（设计 34 第三节「内容包」）：`source` 是 `{"kind": "pack", "package": <包>}`；人改过描述、用法的多一格
//! `edited`，卸了内容包也留下。每个包读进来时 `index.json` 的摘要记在 `packs` 表，没变的不再读。

use std::collections::HashSet;

use rusqlite::{OptionalExtension, params};

use super::{Failed, Library};

/// 某个包带来的、没被人改过的：`json_extract` 照 `source` 的几格挑。
const UNEDITED: &str = "SELECT id, hash FROM memes
    WHERE json_extract(source, '$.kind') = 'pack' AND json_extract(source, '$.package') = ?1
      AND json_extract(source, '$.edited') IS NULL";

impl Library {
    /// 上次读进来时这个包的 `index.json` 的摘要。
    pub fn pack_digest(&self, package: &str) -> Result<Option<String>, Failed> {
        self.db
            .query_row(
                "SELECT digest FROM packs WHERE package = ?1",
                [package],
                |row| row.get(0),
            )
            .optional()
    }

    /// 记下这个包读进来了，摘要是 `digest`。
    pub fn set_pack(&self, package: &str, digest: &str) -> Result<(), Failed> {
        self.db.execute(
            "INSERT INTO packs (package, digest) VALUES (?1, ?2)
             ON CONFLICT(package) DO UPDATE SET digest = excluded.digest",
            params![package, digest],
        )?;
        Ok(())
    }

    /// 读进来过的包。
    pub fn packs(&self) -> Result<Vec<String>, Failed> {
        let mut statement = self
            .db
            .prepare("SELECT package FROM packs ORDER BY package")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect()
    }

    /// 包不在了：它带来的、没被人改过的拿掉，不再记它。交回拿掉了几张。
    pub fn forget_pack(&self, package: &str) -> Result<usize, Failed> {
        let removed = self.prune_pack(package, &HashSet::new())?;
        self.db
            .execute("DELETE FROM packs WHERE package = ?1", [package])?;
        Ok(removed)
    }

    /// 这个包带来的、没被人改过的，图（`hash`）不在 `keep` 里的拿掉。交回拿掉了几张。
    pub fn prune_pack(&self, package: &str, keep: &HashSet<String>) -> Result<usize, Failed> {
        let mut statement = self.db.prepare(UNEDITED)?;
        let found: Vec<(String, String)> = statement
            .query_map([package], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let mut removed = 0;
        for (id, hash) in found {
            if !keep.contains(&hash) && self.purge(&id)? {
                removed += 1;
            }
        }
        Ok(removed)
    }
}
