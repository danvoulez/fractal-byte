
use anyhow::Result;
use cid::Cid;
use std::path::PathBuf;
use tokio::fs;

const STORE_DIR: &str = "store";
const RECEIPT_DIR: &str = "index/receipt";

fn cid_path(cid: &Cid, ext: &str) -> PathBuf {
    let s = cid.to_string();
    let (p1, p2) = (&s[2..4], &s[4..6]);
    PathBuf::from(STORE_DIR).join(p1).join(p2).join(format!("{}.{}", s, ext))
}

fn receipt_path(cid: &Cid) -> PathBuf {
    PathBuf::from(RECEIPT_DIR).join(format!("{}.json", cid))
}

pub async fn put(cid: &Cid, bytes: &[u8]) -> Result<()> {
    let path = cid_path(cid, "nrf");
    fs::create_dir_all(path.parent().unwrap()).await?;
    fs::write(path, bytes).await?;
    Ok(())
}

pub async fn exists(cid: &Cid) -> bool {
    fs::try_exists(cid_path(cid, "nrf")).await.unwrap_or(false)
}

pub async fn get_raw(cid: &Cid) -> Option<Vec<u8>> {
    fs::read(cid_path(cid, "nrf")).await.ok()
}

pub async fn put_receipt(cid: &Cid, bytes: &[u8]) -> Result<()> {
    let path = receipt_path(cid);
    fs::create_dir_all(path.parent().unwrap()).await?;
    fs::write(path, bytes).await?;
    Ok(())
}

pub async fn get_receipt(cid: &Cid) -> Option<Vec<u8>> {
    fs::read(receipt_path(cid)).await.ok()
}

// ── S3 backend (feature-gated) ──────────────────────────────────────

#[cfg(feature = "s3")]
pub mod s3 {
    use anyhow::Result;

    pub struct S3Ledger {
        client: aws_sdk_s3::Client,
        bucket: String,
    }

    impl S3Ledger {
        pub async fn new(bucket: String, region: &str) -> Result<Self> {
            let config = aws_config::from_env()
                .region(aws_config::Region::new(region.to_string()))
                .load()
                .await;
            let client = aws_sdk_s3::Client::new(&config);
            Ok(Self { client, bucket })
        }

        pub async fn put(&self, cid: &str, bytes: &[u8]) -> Result<()> {
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(cid)
                .body(bytes.to_vec().into())
                .send()
                .await?;
            Ok(())
        }

        pub async fn get(&self, cid: &str) -> Option<Vec<u8>> {
            let out = self.client
                .get_object()
                .bucket(&self.bucket)
                .key(cid)
                .send()
                .await
                .ok()?;
            Some(out.body.collect().await.ok()?.into_bytes().to_vec())
        }
    }
}
