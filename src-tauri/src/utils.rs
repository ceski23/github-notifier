use tauri_plugin_http::reqwest;
use temp_file::TempFile;

pub async fn download_icon(url: &str) -> anyhow::Result<TempFile> {
    let icon_content = reqwest::get(url).await?.bytes().await?;
    let icon_file = TempFile::with_suffix(".png")?.with_contents(&icon_content)?;

    Ok(icon_file)
}
