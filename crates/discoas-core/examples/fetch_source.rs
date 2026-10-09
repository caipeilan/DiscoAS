//! Explicit network example: fetch validated metadata, then optionally persist it.
use discoas_core::{core::playlist::TypeName, platforms::fetcher_for, storage::LibraryStore};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&arguments.len()) {
        return Err("用法：fetch_source <NeteaseCloudMusic|QQMusic|KugouMusic|KuwoMusic|QishuiMusic|Spotify|YouTube|Bilibili> <来源类型> <ID> [user_data目录]".into());
    }
    let kind = TypeName::parse(&arguments[1])?;
    let data = fetcher_for(&arguments[0])?
        .fetch(&arguments[2], kind)
        .await?;
    if let Some(root) = arguments.get(3) {
        let (name, count) = LibraryStore::new(root).save_playlist_json(
            &arguments[0],
            &arguments[2],
            kind,
            &data,
        )?;
        println!("已保存：{name}（{count} 首）");
    } else {
        println!("{}", serde_json::to_string_pretty(&data)?);
    }
    Ok(())
}
