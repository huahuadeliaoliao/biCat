use clap::{Arg, Command};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{self, header, Client, ClientBuilder};
use serde::Deserialize;
use serde_json::Value;
use std::fs::File;
use std::io::{self, Cursor};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task;
use tokio::time::sleep;

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
    Ok(headers)
}

#[tokio::main]
async fn main() -> Result<(), ApplicationError> {
    let matches = Command::new("bicat")
        .version("0.1.0")
        .about("Downloads audio from Bilibili given a media ID or BVIDs.")
        .arg(
            Arg::new("media_id")
                .help("The folder ID from which to retrieve the video")
                .required(false)
                .conflicts_with("bvids"),
        )
        .arg(
            Arg::new("bvids")
                .help("One or more BVIDs to download audio from")
                .short('b')
                .required(false)
                .num_args(1..),
        )
        .get_matches();

    let client = ClientBuilder::new()
        .timeout(Duration::from_secs(30))
        .build()?;
    let semaphore = Arc::new(Semaphore::new(10));
    let headers = create_custom_headers().await?;

    let video_bvids = if let Some(bvids) = matches.get_many::<String>("bvids") {
        bvids.map(|s| s.to_string()).collect()
    } else if let Some(media_id) = matches.get_one::<String>("media_id") {
        match fetch_bvids_from_media_id(&client, media_id, &headers).await {
            Ok(bvids) if bvids.is_empty() => {
                eprintln!("Error: No videos found or private collection.");
                return Err(ApplicationError::DataFetchError);
            }
            Ok(bvids) => bvids,
            Err(e) => {
                eprintln!("Data fetch error: {}", e);
                return Err(e);
            }
        }
    } else {
        eprintln!("Input error: Specify media_id or bvids.");
        return Err(ApplicationError::DataFetchError);
    };

    let total_videos = video_bvids.len() as u64;
    let progress_bar = ProgressBar::new(total_videos);
    let style = ProgressStyle::default_bar()
        .template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} ({eta})",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("C>-");
    progress_bar.set_style(style);

    let mut handles = Vec::new();
    for bvid in video_bvids.clone() {
        let client = client.clone();
        let headers = headers.clone();
        let semaphore = semaphore.clone();
        let progress_bar = progress_bar.clone();
        let handle = task::spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            let video_data = fetch_video_data(&client, &bvid, &headers).await?;
            let audio_url =
                fetch_audio_url(&client, &bvid, &video_data.cid.to_string(), &headers).await?;
            download_audio_with_retry(
                &client,
                &audio_url,
                &video_data.title,
                &video_data.owner.name,
                &headers,
                3,
            )
            .await?;
            progress_bar.inc(1);
            Ok::<(), ApplicationError>(())
        });
        handles.push(handle);
    }

    let mut failed_bvids = Vec::new();
    for (handle, bvid) in handles.into_iter().zip(video_bvids.iter()) {
        match handle.await {
            Ok(result) => {
                if let Err(_) = result {
                    eprintln!("Task completed with errors for BVID {}", bvid);
                    failed_bvids.push(bvid.clone());
                }
            }
            Err(_) => {
                eprintln!("Task could not be completed for BVID {}", bvid);
                failed_bvids.push(bvid.clone());
            }
        }
    }

    if !failed_bvids.is_empty() {
        let failed_bvids_str = failed_bvids.join(" ");
        println!(
            "\nFailed to download audio for BVIDs: {}\nuse the \"bicat -b\" command to try again\n",
            failed_bvids_str
        );
        return Err(ApplicationError::TaskProcessingError);
    }

    println!("Download complete");
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

async fn download_audio_with_retry(
    client: &Client,
    audio_url: &str,
    title: &str,
    owner_name: &str,
    headers: &header::HeaderMap,
    retry_limit: usize,
) -> Result<(), ApplicationError> {
    let safe_title = title.replace('/', "-");
    let safe_owner_name = owner_name.replace('/', "-");
    let filename = format!("{}-{}.mp3", safe_title, safe_owner_name);

    for attempt in 0..=retry_limit {
        let response = match client.get(audio_url).headers(headers.clone()).send().await {
            Ok(res) => res,
            Err(e) => {
                if attempt < retry_limit {
                    let wait_time = Duration::from_secs(2u64.pow(attempt as u32));
                    eprintln!(
                        "Attempt {} failed, retrying in {:?}: {}",
                        attempt, wait_time, "Network error"
                    );
                    sleep(wait_time).await;
                    continue;
                } else {
                    return Err(ApplicationError::NetworkError(e));
                }
            }
        };

        let audio_data = match response.bytes().await {
            Ok(data) => data,
            Err(e) => {
                if attempt < retry_limit {
                    let wait_time = Duration::from_secs(2u64.pow(attempt as u32));
                    eprintln!(
                        "Attempt {} failed, retrying in {:?}: {}",
                        attempt, wait_time, "Data download error"
                    );
                    sleep(wait_time).await;
                    continue;
                } else {
                    return Err(ApplicationError::NetworkError(e));
                }
            }
        };

        let mut audio_cursor = Cursor::new(audio_data);
        match File::create(&filename) {
            Ok(mut file) => match io::copy(&mut audio_cursor, &mut file) {
                Ok(_) => return Ok(()),
                Err(e) => {
                    let _ = std::fs::remove_file(&filename);
                    if attempt < retry_limit {
                        let wait_time = Duration::from_secs(2u64.pow(attempt as u32));
                        eprintln!(
                            "Attempt {} failed, retrying in {:?}: {}",
                            attempt, wait_time, "File write error"
                        );
                        sleep(wait_time).await;
                        continue;
                    } else {
                        return Err(ApplicationError::IoError(e));
                    }
                }
            },
            Err(e) => {
                if attempt < retry_limit {
                    let wait_time = Duration::from_secs(2u64.pow(attempt as u32));
                    eprintln!(
                        "Attempt {} failed, retrying in {:?}: {}",
                        attempt, wait_time, "File creation error"
                    );
                    sleep(wait_time).await;
                    continue;
                } else {
                    return Err(ApplicationError::IoError(e));
                }
            }
        };
    }

    Err(ApplicationError::TaskProcessingError)
}
