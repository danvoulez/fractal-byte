
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
