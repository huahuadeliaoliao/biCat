use clap::{Arg, Command};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{self, header, Client, ClientBuilder};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Cursor};
use std::path::Path;
use std::sync::{Arc, Mutex};
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
    headers.insert(
        header::USER_AGENT,
        "Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/115.0".parse()?,
    );
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
    let semaphore = Arc::new(Semaphore::new(50));
    let headers = create_custom_headers().await?;

    // 临时文件路径集合
    let temp_files = Arc::new(Mutex::new(HashSet::new()));

    // 注册信号处理器
    let shutdown_signal = tokio::spawn({
        let temp_files = Arc::clone(&temp_files);
        async move {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for event");
            println!("Received Ctrl+C, shutting down...");
            clean_temp_files(&temp_files);
        }
    });

    let main_task = tokio::spawn(async move {
        if let Err(e) = run_main_logic(matches, client, semaphore, headers, temp_files).await {
            eprintln!("Application error: {}", e);
        }
    });

    tokio::select! {
        _ = shutdown_signal => {
            println!("Cleanup before exiting...");
        },
        _ = main_task => {}
    }

    Ok(())
}

async fn run_main_logic(
    matches: clap::ArgMatches,
    client: Client,
    semaphore: Arc<Semaphore>,
    headers: header::HeaderMap,
    temp_files: Arc<Mutex<HashSet<String>>>,
) -> Result<(), ApplicationError> {
    let video_bvids = if let Some(bvids) = matches.get_many::<String>("bvids") {
        bvids.map(|s| s.to_string()).collect()
    } else if let Some(media_id) = matches.get_one::<String>("media_id") {
        match fetch_bvids_from_media_id(&client, media_id, &headers).await {
            Ok(bvids) if bvids.is_empty() => {
                eprintln!("Error: No videos found or private collection.");
                return Err(ApplicationError::DataFetchError);
            }
            Ok(bvids) => {
                if let Err(e) = create_and_enter_directory(media_id) {
                    eprintln!("Directory creation error: {}", e);
                    return Err(ApplicationError::IoError(e));
                }
                bvids
            }
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
        let temp_files = Arc::clone(&temp_files);
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
                &temp_files,
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

    clean_temp_files(&temp_files);

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

fn clean_temp_files(temp_files: &Arc<Mutex<HashSet<String>>>) {
    let temp_files = temp_files.lock().unwrap();
    for temp_file in temp_files.iter() {
        let _ = fs::remove_file(temp_file);
    }
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
    temp_files: &Arc<Mutex<HashSet<String>>>,
) -> Result<(), ApplicationError> {
    let safe_title = title.replace('/', "-");
    let safe_owner_name = owner_name.replace('/', "-");
    let filename = format!("{}-{}.mp3", safe_title, safe_owner_name);
    let temp_filename = format!("{}.tmp", filename);

    {
        let mut temp_files_guard = temp_files.lock().unwrap();
        temp_files_guard.insert(temp_filename.clone());
    }

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
        match File::create(&temp_filename) {
            Ok(mut file) => match io::copy(&mut audio_cursor, &mut file) {
                Ok(_) => {
                    fs::rename(&temp_filename, &filename)?;
                    {
                        let mut temp_files_guard = temp_files.lock().unwrap();
                        temp_files_guard.remove(&temp_filename);
                    }
                    return Ok(());
                }
                Err(e) => {
                    let _ = fs::remove_file(&temp_filename);
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

fn create_and_enter_directory(media_id: &str) -> Result<(), io::Error> {
    let dir_path = Path::new(media_id);
    if dir_path.exists() {
        println!(
            "Directory {} already exists. Do you want to overwrite it? (y/n)",
            media_id
        );
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim().eq_ignore_ascii_case("y") {
            fs::remove_dir_all(dir_path)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Directory already exists",
            ));
        }
    }
    fs::create_dir(media_id)?;
    std::env::set_current_dir(media_id)?;
    Ok(())
}
