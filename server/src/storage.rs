//! The data directory, optionally kept in an S3 bucket (R2, MinIO, AWS and
//! the like) through LEGEJO_STORAGE=s3://bucket/prefix.
//!
//! Everything in Legejo works on files in the data directory, as before. With
//! a bucket, the directories that cannot be made again (book files, covers,
//! audiobooks, avatars, exports) are also kept there: a file written to them
//! is uploaded at once (`persist`), one removed is removed there (`remove`),
//! and a file that is not on the disk is fetched before it is read (`fetch`).
//! The disk is then a cache: a new container with an empty data directory
//! has the whole library, as long as the database is elsewhere (PostgreSQL).
//! What is made from the files (KEPUB copies, thumbnails, the cache) stays on
//! the disk only and is made again when needed.
//!
//! Credentials come from the usual AWS variables, which every S3-compatible
//! service accepts: AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION
//! ("auto" for R2) and AWS_ENDPOINT_URL for anything but AWS itself.

use object_store::path::Path as Key;
use object_store::{ObjectStore, ObjectStoreExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The directories of the data directory that are kept in the bucket.
pub const DURABLE: [&str; 7] = ["books", "covers", "shelf_covers", "audio", "audio_covers", "avatars", "exports"];

pub struct Storage {
    root: PathBuf,
    remote: Option<Remote>,
    /// Tests: empty the durable directories on the disk after every request,
    /// so that a file written but not kept in the bucket, or read without
    /// being fetched, shows.
    #[cfg(test)]
    forget: bool,
}

struct Remote {
    store: Arc<dyn ObjectStore>,
    /// Inside the bucket, before every key ("" for the bucket's root).
    prefix: String,
    /// "s3://bucket/prefix", for the log.
    label: String,
}

impl Storage {
    /// The data directory alone, as before.
    pub fn local(root: PathBuf) -> Storage {
        Storage { root, remote: None, #[cfg(test)] forget: false }
    }

    /// From LEGEJO_STORAGE; the data directory alone when it is unset.
    pub fn from_env(root: PathBuf) -> anyhow::Result<Storage> {
        let Some(url) = crate::settings::var("LEGEJO_STORAGE")? else {
            return Ok(Storage::local(root));
        };
        let url = url.trim().to_string();
        let rest = url
            .strip_prefix("s3://")
            .ok_or_else(|| anyhow::anyhow!("LEGEJO_STORAGE must be s3://bucket or s3://bucket/prefix, not {url}"))?;
        let (bucket, prefix) = rest.split_once('/').unwrap_or((rest, ""));
        anyhow::ensure!(!bucket.is_empty(), "LEGEJO_STORAGE names no bucket");
        let mut builder = object_store::aws::AmazonS3Builder::from_env().with_bucket_name(bucket);
        // AWS_ENDPOINT_URL is what the AWS tools read; object_store reads
        // AWS_ENDPOINT. Either will do.
        if let Some(endpoint) = crate::settings::var("AWS_ENDPOINT_URL")? {
            builder = builder.with_endpoint(endpoint.trim());
        }
        let endpoint_http = crate::settings::var("AWS_ENDPOINT_URL")?.or(crate::settings::var("AWS_ENDPOINT")?).is_some_and(|e| e.trim().starts_with("http://"));
        if endpoint_http {
            builder = builder.with_allow_http(true);
        }
        let store = builder.build()?;
        Ok(Storage::with_store(root, Arc::new(store), prefix, &url))
    }

    /// With a store of one's own (tests use one in memory).
    pub fn with_store(root: PathBuf, store: Arc<dyn ObjectStore>, prefix: &str, label: &str) -> Storage {
        let prefix = prefix.trim_matches('/').to_string();
        Storage { root, remote: Some(Remote { store, prefix, label: label.to_string() }), #[cfg(test)] forget: false }
    }

    pub fn remote_label(&self) -> Option<&str> {
        self.remote.as_ref().map(|r| r.label.as_str())
    }

    /// The key of a file in the data directory, when it is kept in the bucket.
    fn key(&self, path: &Path) -> Option<Key> {
        let remote = self.remote.as_ref()?;
        let rel = path.strip_prefix(&self.root).ok()?;
        let first = rel.components().next()?.as_os_str().to_str()?;
        if !DURABLE.contains(&first) {
            return None;
        }
        let rel = rel.to_str()?.replace('\\', "/");
        Some(Key::from(if remote.prefix.is_empty() { rel } else { format!("{}/{rel}", remote.prefix) }))
    }

    /// Make sure a file is on the disk, fetching it from the bucket when it
    /// is not. True when the file is there afterwards; false for a file that
    /// is in neither place (the caller's "not found").
    pub async fn fetch(&self, path: &Path) -> bool {
        if tokio::fs::try_exists(path).await.unwrap_or(false) {
            return true;
        }
        let (Some(remote), Some(key)) = (self.remote.as_ref(), self.key(path)) else {
            return false;
        };
        match self.download(remote, &key, path).await {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!("storage: fetching {key} failed: {e:#}");
                false
            }
        }
    }

    async fn download(&self, remote: &Remote, key: &Key, path: &Path) -> anyhow::Result<bool> {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;
        let got = match remote.store.get(key).await {
            Ok(got) => got,
            Err(object_store::Error::NotFound { .. }) => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        // Written beside the file and renamed into place, so that a reader
        // never sees half of it.
        let tmp = self.root.join("tmp");
        tokio::fs::create_dir_all(&tmp).await?;
        let part = tmp.join(format!("fetch-{}", crate::books::new_uuid()));
        let mut file = tokio::fs::File::create(&part).await?;
        let mut stream = got.into_stream();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => file.write_all(&bytes).await?,
                Err(e) => {
                    let _ = tokio::fs::remove_file(&part).await;
                    return Err(e.into());
                }
            }
        }
        file.flush().await?;
        drop(file);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::rename(&part, path).await?;
        Ok(true)
    }

    /// The size of a file that is only in the bucket; None when it is on the
    /// disk (read it there) or nowhere.
    pub async fn remote_size(&self, path: &Path) -> Option<u64> {
        if tokio::fs::try_exists(path).await.unwrap_or(false) {
            return None;
        }
        let (remote, key) = (self.remote.as_ref()?, self.key(path)?);
        remote.store.head(&key).await.ok().map(|m| m.size)
    }

    /// Bytes `start..end` of a file in the bucket, as a stream: an audiobook
    /// is played a minute at a time, and is not fetched whole for that.
    pub async fn remote_range(
        &self,
        path: &Path,
        start: u64,
        end: u64,
    ) -> anyhow::Result<impl futures_util::Stream<Item = std::io::Result<bytes::Bytes>> + Send + 'static> {
        use futures_util::StreamExt;
        let remote = self.remote.as_ref().ok_or_else(|| anyhow::anyhow!("no bucket"))?;
        let key = self.key(path).ok_or_else(|| anyhow::anyhow!("not kept in the bucket"))?;
        let options = object_store::GetOptions { range: Some((start..end).into()), ..Default::default() };
        let got = remote.store.get_opts(&key, options).await?;
        Ok(got.into_stream().map(|chunk| chunk.map_err(std::io::Error::other)))
    }

    /// Keep a file just written in the bucket too. An error means the file
    /// is only on the disk, and the caller should fail what it was doing.
    pub async fn persist(&self, path: &Path) -> anyhow::Result<()> {
        let (Some(remote), Some(key)) = (self.remote.as_ref(), self.key(path)) else {
            return Ok(());
        };
        let mut file = tokio::fs::File::open(path).await?;
        // Large files go up in parts; small ones in one request.
        let mut writer = object_store::buffered::BufWriter::new(remote.store.clone(), key.clone());
        tokio::io::copy(&mut file, &mut writer).await?;
        tokio::io::AsyncWriteExt::shutdown(&mut writer).await?;
        Ok(())
    }

    /// Remove a file from the bucket as well; one that is not there is no error.
    pub async fn remove(&self, path: &Path) {
        let (Some(remote), Some(key)) = (self.remote.as_ref(), self.key(path)) else {
            return;
        };
        match remote.store.delete(&key).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => {}
            Err(e) => tracing::warn!("storage: removing {key} failed: {e:#}"),
        }
    }

    /// Remove everything under a directory from the bucket (an audiobook's
    /// parts, an export).
    pub async fn remove_dir(&self, dir: &Path) {
        use futures_util::StreamExt;
        let (Some(remote), Some(key)) = (self.remote.as_ref(), self.key(dir)) else {
            return;
        };
        let keys: Vec<Key> = remote.store.list(Some(&key)).filter_map(|m| async move { m.ok().map(|m| m.location) }).collect().await;
        for k in keys {
            if let Err(e) = remote.store.delete(&k).await {
                tracing::warn!("storage: removing {k} failed: {e:#}");
            }
        }
    }

    /// The files of a directory, by name: on the disk and, with a bucket,
    /// there too (an audiobook's parts on a fresh disk).
    pub async fn list_dir(&self, dir: &Path) -> Vec<String> {
        use futures_util::StreamExt;
        let mut names = std::collections::BTreeSet::new();
        if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Some(name) = entry.file_name().to_str() {
                    names.insert(name.to_string());
                }
            }
        }
        if let (Some(remote), Some(key)) = (self.remote.as_ref(), self.key(dir)) {
            let found: Vec<Key> = remote.store.list(Some(&key)).filter_map(|m| async move { m.ok().map(|m| m.location) }).collect().await;
            for k in found {
                if let Some(name) = k.filename() {
                    names.insert(name.to_string());
                }
            }
        }
        names.into_iter().collect()
    }

    /// `legejo storage migrate`: put every durable file of the data directory
    /// in the bucket, skipping those already there with the same size.
    /// Returns (uploaded, already there).
    pub async fn migrate(&self) -> anyhow::Result<(usize, usize)> {
        let remote = self.remote.as_ref().ok_or_else(|| anyhow::anyhow!("LEGEJO_STORAGE is not set"))?;
        let (mut uploaded, mut present) = (0, 0);
        let mut stack: Vec<PathBuf> = DURABLE.iter().map(|d| self.root.join(d)).collect();
        while let Some(dir) = stack.pop() {
            let Ok(mut entries) = tokio::fs::read_dir(&dir).await else { continue };
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let meta = entry.metadata().await?;
                if meta.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Some(key) = self.key(&path) else { continue };
                match remote.store.head(&key).await {
                    Ok(head) if head.size == meta.len() => present += 1,
                    _ => {
                        self.persist(&path).await?;
                        uploaded += 1;
                        if uploaded % 100 == 0 {
                            println!("  {uploaded} files uploaded");
                        }
                    }
                }
            }
        }
        Ok((uploaded, present))
    }
}

#[cfg(test)]
impl Storage {
    /// A bucket in memory, and a disk that forgets after every request.
    pub fn forgetful(root: PathBuf) -> Storage {
        let store = Arc::new(object_store::memory::InMemory::new());
        let mut storage = Storage::with_store(root, store, "library", "memory://library");
        storage.forget = true;
        storage
    }

    pub fn forgets(&self) -> bool {
        self.forget
    }

    /// Empty the durable directories on the disk; the bucket keeps them.
    pub fn forget_disk(&self) {
        for dir in DURABLE {
            let path = self.root.join(dir);
            let _ = std::fs::remove_dir_all(&path);
            let _ = std::fs::create_dir_all(&path);
        }
    }
}

#[cfg(test)]
pub async fn forget_after(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let res = next.run(req).await;
    state.storage.forget_disk();
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_file_lives_in_the_bucket_and_comes_back_to_an_empty_disk() {
        let root = std::env::temp_dir().join(format!("legejo-storage-{}", crate::books::new_uuid()));
        let storage = Storage::forgetful(root.clone());
        let book = root.join("books").join("abc.epub");
        let thumb = root.join("thumbs").join("abc.jpg");
        std::fs::create_dir_all(book.parent().unwrap()).unwrap();
        std::fs::create_dir_all(thumb.parent().unwrap()).unwrap();
        std::fs::write(&book, b"the book").unwrap();
        std::fs::write(&thumb, b"a thumbnail").unwrap();
        storage.persist(&book).await.unwrap();
        // What is made from the files stays on the disk only.
        storage.persist(&thumb).await.unwrap();

        storage.forget_disk();
        assert!(!book.exists());
        assert_eq!(storage.remote_size(&book).await, Some(8));
        let part: Vec<u8> = {
            use futures_util::StreamExt;
            let chunks: Vec<_> = storage.remote_range(&book, 4, 8).await.unwrap().collect().await;
            chunks.into_iter().flat_map(|c| c.unwrap().to_vec()).collect()
        };
        assert_eq!(part, b"book");
        assert!(storage.fetch(&book).await);
        assert_eq!(std::fs::read(&book).unwrap(), b"the book");
        assert_eq!(storage.remote_size(&book).await, None, "on the disk again");

        std::fs::remove_file(&thumb).unwrap();
        assert!(!storage.fetch(&thumb).await, "a thumbnail is never in the bucket");

        storage.remove(&book).await;
        std::fs::remove_file(&book).unwrap();
        assert!(!storage.fetch(&book).await);
    }

    #[tokio::test]
    async fn migrate_puts_the_data_directory_in_the_bucket_once() {
        let root = std::env::temp_dir().join(format!("legejo-storage-{}", crate::books::new_uuid()));
        let storage = Storage::forgetful(root.clone());
        for (dir, name) in [("books", "a.epub"), ("covers", "a"), ("audio/x", "1.mp3"), ("thumbs", "a.jpg")] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join(name), name).unwrap();
        }
        assert_eq!(storage.migrate().await.unwrap(), (3, 0), "the thumbnail stays behind");
        assert_eq!(storage.migrate().await.unwrap(), (0, 3), "and nothing twice");
        storage.forget_disk();
        assert!(storage.fetch(&root.join("audio/x/1.mp3")).await);
    }
}
