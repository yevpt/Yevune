//! FFmpeg 子进程与 Garage 输入/实时输出管线。

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

use crate::storage::{ObjectStore, StorageError, STREAM_CHUNK_SIZE};

use super::stream::{send_chunk, OutputSender};
use super::{Error, Result, TranscodeTarget, TranscodeTrack};

pub(crate) async fn run(
    store: Arc<dyn ObjectStore>,
    ffmpeg_path: &Path,
    track: &TranscodeTrack,
    target: &TranscodeTarget,
    tx: &OutputSender,
) -> Result<Option<tempfile::NamedTempFile>> {
    let temp = tempfile::NamedTempFile::new()?;
    let output_file = temp.reopen()?;
    let mut output_file = tokio::fs::File::from_std(output_file);
    let mut child = spawn(ffmpeg_path, target)?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::Task("FFmpeg stdin 未创建".into()))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Task("FFmpeg stdout 未创建".into()))?;
    let input_store = store.clone();
    let input_key = track.object_key.clone();
    let mut feeder = tokio::spawn(async move { feed_input(input_store, input_key, stdin).await });
    let mut chunk = vec![0_u8; STREAM_CHUNK_SIZE];

    loop {
        let read_result = tokio::select! {
            _ = tx.closed() => {
                abort_child(&mut child, &feeder).await;
                return Ok(None);
            }
            result = stdout.read(&mut chunk) => result,
        };
        let read = match read_result {
            Ok(read) => read,
            Err(error) => {
                abort_child(&mut child, &feeder).await;
                return Err(error.into());
            }
        };
        if read == 0 {
            break;
        }
        if let Err(error) = output_file.write_all(&chunk[..read]).await {
            abort_child(&mut child, &feeder).await;
            return Err(error.into());
        }
        if send_chunk(tx, Bytes::copy_from_slice(&chunk[..read]))
            .await
            .is_err()
        {
            abort_child(&mut child, &feeder).await;
            return Ok(None);
        }
    }

    let feeder_result = tokio::select! {
        _ = tx.closed() => {
            abort_child(&mut child, &feeder).await;
            return Ok(None);
        }
        result = &mut feeder => result,
    };
    match feeder_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error);
        }
        Err(error) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(Error::Task(error.to_string()));
        }
    }
    let status = tokio::select! {
        _ = tx.closed() => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Ok(None);
        }
        result = child.wait() => result?,
    };
    if !status.success() {
        return Err(Error::FfmpegFailed);
    }
    output_file.flush().await?;
    drop(output_file);
    Ok(Some(temp))
}

fn spawn(path: &Path, target: &TranscodeTarget) -> Result<Child> {
    let (codec, container) = match target.format.as_str() {
        "aac" => ("aac", "adts"),
        "opus" => ("libopus", "ogg"),
        other => return Err(Error::UnsupportedFormat(other.to_string())),
    };
    let mut command = Command::new(path);
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            "pipe:0",
            "-map",
            "0:a:0",
            "-vn",
            "-c:a",
            codec,
            "-b:a",
            &format!("{}k", target.bitrate),
            "-f",
            container,
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command.spawn().map_err(Error::Io)
}

async fn feed_input(
    store: Arc<dyn ObjectStore>,
    key: String,
    mut stdin: tokio::process::ChildStdin,
) -> Result<()> {
    let size = store.head(&key).await?.size;
    let mut offset = 0_u64;
    while offset < size {
        let end = (offset + STREAM_CHUNK_SIZE as u64).min(size);
        let bytes = store.get_range(&key, offset..end).await?;
        if bytes.is_empty() {
            return Err(Error::Storage(StorageError::Backend(format!(
                "对象 {key} 在 {offset} 字节提前结束"
            ))));
        }
        stdin.write_all(&bytes).await?;
        offset += bytes.len() as u64;
    }
    stdin.shutdown().await?;
    Ok(())
}

async fn abort_child(child: &mut Child, feeder: &tokio::task::JoinHandle<Result<()>>) {
    feeder.abort();
    let _ = child.kill().await;
    let _ = child.wait().await;
}
