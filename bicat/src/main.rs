use reqwest::{self, header, Client, Error};
use serde::Deserialize;
use serde_json::Value;
use std::fs::File;
use std::io::{self, Cursor};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task;

const BASE_API_URL: &str = "https://api.bilibili.com/x/player/playurl?fnval=16";

#[derive(Deserialize)]
struct VideoData {
    title: String,
    cid: i64,
}

#[derive(Deserialize)]
struct ApiResponse<T> {
    data: T,
}

async fn create_custom_headers() -> Result<header::HeaderMap, header::InvalidHeaderValue> {
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
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let media_id = "3124599534";
    let client = Client::new();
    let semaphore = Arc::new(Semaphore::new(10)); // 最多同时10个任务
    let headers = create_custom_headers().await?;

    let video_bvids = fetch_bvids_from_media_id(&client, media_id, &headers).await?;
    let mut handles = Vec::new();

    for bvid in video_bvids {
        let client = client.clone();
        let headers = headers.clone();
        let permit = semaphore.clone().acquire_owned().await?;
        let handle = task::spawn(async move {
            let video_data = fetch_video_data(&client, &bvid, &headers).await;
            if let Ok(data) = video_data {
                let audio_url =
                    fetch_audio_url(&client, &bvid, &data.cid.to_string(), &headers).await;
                if let Ok(url) = audio_url {
                    download_audio(&client, &url, &data.title, &headers)
                        .await
                        .ok();
                }
            }
            drop(permit); // 释放信号量
        });
        handles.push(handle);
    }

    // 等待所有任务完成
    for handle in handles {
        handle.await?;
    }

    Ok(())
}

async fn fetch_bvids_from_media_id(
    client: &Client,
    media_id: &str,
    headers: &header::HeaderMap,
) -> Result<Vec<String>, Error> {
    let response = client
        .get("https://api.bilibili.com/x/v3/fav/resource/ids")
        .headers(headers.clone())
        .query(&[("media_id", media_id), ("platform", "web")])
        .send()
        .await?
        .json::<Value>()
        .await?;

    let bvids = response["data"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["bv_id"].as_str().map(|s| s.to_string()))
        .collect();

    Ok(bvids)
}

async fn fetch_video_data(
    client: &Client,
    bvid: &str,
    headers: &header::HeaderMap,
) -> Result<VideoData, Error> {
    let url = format!(
        "https://api.bilibili.com/x/web-interface/view?bvid={}",
        bvid
    );
    let response = client
        .get(&url)
        .headers(headers.clone())
        .send()
        .await?
        .json::<ApiResponse<VideoData>>()
        .await?;
    Ok(response.data)
}

async fn fetch_audio_url(
    client: &Client,
    bvid: &str,
    cid: &str,
    headers: &header::HeaderMap,
) -> Result<String, Error> {
    let url = format!("{}&bvid={}&cid={}", BASE_API_URL, bvid, cid);
    client
        .get(&url)
        .headers(headers.clone())
        .send()
        .await?
        .json::<Value>()
        .await
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
    headers: &header::HeaderMap,
) -> io::Result<()> {
    let response = client
        .get(audio_url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let audio_data = response
        .bytes()
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let mut audio_cursor = Cursor::new(audio_data);
    let mut file = File::create(format!("{}.mp3", title.replace('/', "-")))?; // Replace to avoid path issues
    io::copy(&mut audio_cursor, &mut file)?;
    Ok(())
}
