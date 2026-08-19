//! Cross-platform file system scanner
//! 
//! Automatically uses Win32 API on Windows and walkdir+rayon on Unix/macOS.

pub mod platform;

use crate::cache::Cache;
pub use crate::scanner_types::*;
pub use crate::scanner::platform::Progress;
use crate::scanner::platform::{PlatformWalker, create_walker};

/// High-level scanner that delegates to platform-specific implementation
pub struct Scanner {
    walker: std::sync::Arc<Box<dyn PlatformWalker>>,
}

impl Scanner {
    pub fn new(opts: Options, progress: Progress, cache: Option<Arc<dyn Cache>>) -> Self {
        let walker = create_walker(opts, progress, cache);
        Self { walker: std::sync::Arc::new(walker) }
    }

    pub fn walk(&self, root: &str) -> anyhow::Result<(Vec<FileRecord>, Vec<ScanError>)> {
        self.walker.walk(root)
    }

    pub fn stop(&self) {
        self.walker.stop();
    }
}

impl Clone for Scanner {
    fn clone(&self) -> Self {
        Self { walker: self.walker.clone() }
    }
}

use std::sync::Arc;