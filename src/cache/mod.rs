//! Incremental cache for directory fingerprints using redb (embedded key-value store)

use std::path::PathBuf;
use std::sync::Arc;
use redb::{Database, TableDefinition, ReadableTable};
use anyhow::Result;

use crate::scanner_types::{CacheEntry, Fingerprint, Options as ScannerOptions};

const CACHE_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("cache");
const META_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("meta");

/// Cache trait for incremental scanning
pub trait Cache: Send + Sync {
    #[allow(dead_code)]
    fn lookup(&self, key: &str, fp: &Fingerprint) -> Option<CacheEntry>;
    #[allow(dead_code)]
    fn save(&self, key: &str, entry: CacheEntry) -> Result<()>;
    fn save_total(&self, root_key: &str, n: i64) -> Result<()>;
    fn load_total(&self, root_key: &str) -> Option<i64>;
}

/// Redb-based cache implementation
pub struct BoltCache {
    db: Arc<Database>,
    #[allow(dead_code)]
    generation: String,
}

impl BoltCache {
    pub fn new(app_name: &str, config_hash: &str) -> Result<Self> {
        let cache_dir = if let Some(local) = dirs::cache_dir() {
            local.join(app_name)
        } else {
            PathBuf::from(".").join(".cache").join(app_name)
        };

        std::fs::create_dir_all(&cache_dir)?;
        let db_path = cache_dir.join("scan_cache.redb");

        let db = Arc::new(Database::create(&db_path)?);

        // Initialize tables and check generation
        {
            let write_txn = db.begin_write()?;
            {
                let mut meta = write_txn.open_table(META_TABLE)?;
                let gen_key = "generation";
                let stored_gen: Option<Vec<u8>> = meta.get(gen_key.as_bytes())?.map(|v| v.value().to_vec());
                
                if stored_gen.as_deref() == Some(config_hash.as_bytes()) {
                    // Generation matches, keep existing cache
                } else {
                    // Generation mismatch - clear cache
                    if write_txn.open_table(CACHE_TABLE).is_ok() {
                        write_txn.delete_table(CACHE_TABLE)?;
                    }
                    let _ = write_txn.open_table(CACHE_TABLE)?;
                    meta.insert(gen_key.as_bytes(), config_hash.as_bytes())?;
                }
            }
            write_txn.commit()?;
        }

        Ok(Self { db, generation: config_hash.to_string() })
    }
}

impl Cache for BoltCache {
    fn lookup(&self, key: &str, fp: &Fingerprint) -> Option<CacheEntry> {
        let read_txn = self.db.begin_read().ok()?;
        let table = read_txn.open_table(CACHE_TABLE).ok()?;
        let data = table.get(key.as_bytes()).ok()??;
        let entry: CacheEntry = postcard::from_bytes(data.value()).ok()?;
        
        if entry.fingerprint.mod_time_ns == fp.mod_time_ns {
            Some(entry)
        } else {
            None
        }
    }

    fn save(&self, key: &str, entry: CacheEntry) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CACHE_TABLE)?;
            let data = postcard::to_allocvec(&entry)?;
            table.insert(key.as_bytes(), data.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn save_total(&self, root_key: &str, n: i64) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        { 
            let mut table = write_txn.open_table(META_TABLE)?; 
            let key = format!("total:{root_key}");
            let value = n.to_le_bytes();
            table.insert(key.as_bytes(), value.as_slice())?; 
        }
        write_txn.commit()?;
        Ok(())
    }

    fn load_total(&self, root_key: &str) -> Option<i64> {
        let read_txn = self.db.begin_read().ok()?;
        let table = read_txn.open_table(META_TABLE).ok()?;
        let key = format!("total:{root_key}");
        let data = table.get(key.as_bytes()).ok()??;
        let bytes: [u8; 8] = data.value().try_into().ok()?;
        Some(i64::from_le_bytes(bytes))
    }
}

/// Compute a simple hash of configuration for cache invalidation
#[allow(dead_code)]
pub fn config_hash(opts: &ScannerOptions) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    opts.workers.hash(&mut hasher);
    opts.follow_links.hash(&mut hasher);
    for e in &opts.exclude { e.hash(&mut hasher); }
    for p in &opts.exclude_pref { p.hash(&mut hasher); }
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::scanner_types::{FileRecord, Fingerprint, Attrs};
    use std::time::SystemTime;

    #[test]
    fn test_cache_roundtrip() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.redb");
        let db = Database::create(&db_path).unwrap();
        
        let cache = BoltCache { db: Arc::new(db), generation: "test".to_string() };

        let entry = CacheEntry {
            fingerprint: Fingerprint { mod_time_ns: 12345 },
            files: vec![
                FileRecord { path: "C:\\test\\file.txt".to_string(), size: 1024, mod_time: SystemTime::now(), attrs: Attrs { is_dir: false, is_reparse: false, is_hidden: false, is_system: false } }
            ],
            dirs: vec!["subdir".to_string()],
        };

        cache.save("C:\\test", entry.clone()).unwrap();
        let loaded = cache.lookup("C:\\test", &Fingerprint { mod_time_ns: 12345 }).unwrap();
        
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].path, "C:\\test\\file.txt");

        cache.save_total("c:\\", 10).unwrap();
        cache.save_total("d:\\", 20).unwrap();
        assert_eq!(cache.load_total("c:\\"), Some(10));
        assert_eq!(cache.load_total("d:\\"), Some(20));
    }
}
