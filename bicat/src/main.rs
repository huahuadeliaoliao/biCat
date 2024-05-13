use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{self, header, Client};
use serde::Deserialize;
use serde_json::Value;
use std::fs::File;
use std::io::{self, Cursor};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task;

mod error;
use crate::error::ApplicationError;

const BASE_API_URL: &str = "https://api.bilibili.com/x/player/playurl?fnval=16";

#[derive(Deserialize)]
struct Owner {
    name: String,
}

#[derive(Deserialize)]
struct VideoData {
    title: String,
    cid: i64,
    owner: Owner,
}

#[derive(Deserialize)]
struct ApiResponse<T> {
    data: T,
}

async fn create_custom_headers() -> Result<header::HeaderMap, ApplicationError> {
    let mut headers = header::HeaderMap::new();
    headers.insert(header::CONNECTION, "Keep-Alive".parse()?);
    headers.insert(
        header::ACCEPT_LANGUAGE,
        "en-US,en;q=0.8,zh-Hans-CN;q=0.5,zh-Hans;q=0.3".parse()?,
    );
    headers.insert(
        header::ACCEPT,
        "text/html, application/xhtml+xml, */*".parse()?,
    );
    headers.insert(header::REFERER, "https://www.bilibili.com".parse()?);
    headers.insert(
        header::USER_AGENT,
        "Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/115.0".parse()?,
    );
    Ok(headers)
}

#[tokio::main]
async fn main() -> Result<(), ApplicationError> {
    let client = Client::new();
    let semaphore = Arc::new(Semaphore::new(10));
    let headers = create_custom_headers().await?;
    let media_id = "3124599534";
    let video_bvids = fetch_bvids_from_media_id(&client, media_id, &headers).await?;
    let total_videos = video_bvids.len() as u64;

    let progress_bar = ProgressBar::new(total_videos);
    let style = ProgressStyle::default_bar()
        .template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} ({eta})",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar());
    progress_bar.set_style(style.progress_chars("C>-"));

    let mut handles = Vec::new();
    for bvid in video_bvids {
        let client = client.clone();
        let headers = headers.clone();
        let semaphore = semaphore.clone();
        let progress_bar = progress_bar.clone();
        let handle = task::spawn(async move {
            let permit = semaphore.acquire_owned().await?;
            let video_data = fetch_video_data(&client, &bvid, &headers).await?;
            let audio_url =
                fetch_audio_url(&client, &bvid, &video_data.cid.to_string(), &headers).await?;
            download_audio(
                &client,
                &audio_url,
                &video_data.title,
                &video_data.owner.name,
                &headers,
            )
            .await?;
            progress_bar.inc(1);
            drop(permit);
            Ok::<(), ApplicationError>(())
        });
        handles.push(handle);
    }

    for handle in handles {
        if let Err(_) = handle.await? {
            return Err(ApplicationError::TaskProcessingError);
        }
    }

    progress_bar.finish_with_message("Download complete");
    Ok(())
}

async fn fetch_bvids_from_media_id(
    client: &Client,
    media_id: &str,
    headers: &header::HeaderMap,
) -> Result<Vec<String>, ApplicationError> {
    let response = client
        .get("https://api.bilibili.com/x/v3/fav/resource/ids")
        .headers(headers.clone())
        .query(&[("media_id", media_id), ("platform", "web")])
        .send()
        .await
        .map_err(ApplicationError::NetworkError)?;
    let json = response
        .json::<Value>()
        .await
        .map_err(|_| ApplicationError::DataParsingError("Invalid JSON format.".to_string()))?;
    let bvids = json["data"]
        .as_array()
        .ok_or(ApplicationError::DataFetchError)?
        .iter()
        .filter_map(|v| v["bv_id"].as_str().map(String::from))
        .collect();
    Ok(bvids)
}

async fn fetch_video_data(
    client: &Client,
    bvid: &str,
    headers: &header::HeaderMap,
) -> Result<VideoData, ApplicationError> {
    let url = format!(
        "https://api.bilibili.com/x/web-interface/view?bvid={}",
        bvid
    );
    let response = client
        .get(&url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(ApplicationError::NetworkError)?;
    response
        .json::<ApiResponse<VideoData>>()
        .await
        .map(|api_response| api_response.data)
        .map_err(|_| ApplicationError::DataParsingError("Failed to parse video data.".to_string()))
}

async fn fetch_audio_url(
    client: &Client,
    bvid: &str,
    cid: &str,
    headers: &header::HeaderMap,
) -> Result<String, ApplicationError> {
    let url = format!("{}&bvid={}&cid={}", BASE_API_URL, bvid, cid);
    let response = client
        .get(&url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(ApplicationError::NetworkError)?;
    response
        .json::<Value>()
        .await
        .map_err(|_| ApplicationError::DataParsingError("Failed to parse audio URL.".to_string()))
        .map(|json| {
            json["data"]["dash"]["audio"][0]["baseUrl"]
                .as_str()
                .unwrap_or("")
                .to_string()
        })
}

async fn download_audio(
    client: &Client,
    audio_url: &str,
    title: &str,
    owner_name: &str,
    headers: &header::HeaderMap,
) -> Result<(), ApplicationError> {
    let safe_title = title.replace('/', "-");
    let safe_owner_name = owner_name.replace('/', "-");
    let filename = format!("{}-{}.mp3", safe_title, safe_owner_name);
    let response = client
        .get(audio_url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(ApplicationError::NetworkError)?;

    let audio_data = response
        .bytes()
        .await
        .map_err(ApplicationError::NetworkError)?;

    let mut audio_cursor = Cursor::new(audio_data);
    let file_result = File::create(&filename);

    match file_result {
        Ok(mut file) => {
            if let Err(e) =
                io::copy(&mut audio_cursor, &mut file).map_err(ApplicationError::IoError)
            {
                let _ = std::fs::remove_file(&filename);
                Err(e)
            } else {
                Ok(())
            }
        }
        Err(e) => Err(ApplicationError::IoError(e)),
    }
}
