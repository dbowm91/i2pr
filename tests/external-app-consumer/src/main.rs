use std::{env, fs::OpenOptions, io::Write, path::PathBuf};

use i2pr_app_sdk::{
    Session,
    protocol::{AppId, AppService},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app_id = None;
    let mut instance = None;
    for argument in env::args().skip(1) {
        if let Some(value) = argument.strip_prefix("--i2pr-app-id=") {
            app_id = Some(value.to_owned());
        }
        if let Some(value) = argument.strip_prefix("--i2pr-app-instance=") {
            instance = Some(value.to_owned());
        }
    }
    let app_id_text = app_id.ok_or("managed host omitted app id")?;
    let instance = instance.ok_or("managed host omitted instance id")?;
    let data_root = PathBuf::from(
        env::var_os("I2PR_APP_DATA_DIR").ok_or("managed host omitted app data root")?,
    );
    let transcript = data_root.join(format!("transcript-{app_id_text}-{instance}.jsonl"));
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(transcript)?;
    note(
        &mut log,
        "start",
        serde_json::json!({"app_id":app_id_text,"instance":instance,"scenario":"external-sdk"}),
    )?;
    let app_id = AppId::parse(app_id_text.clone())?;
    let reader = BlockingStdin(std::io::stdin());
    let writer = BlockingStdout(std::io::stdout());
    futures_executor::block_on(async {
        let mut session = Session::connect(reader, writer, app_id, instance).await?;
        let capabilities = session
            .capabilities()
            .map(|item| format!("{item:?}"))
            .collect::<Vec<_>>();
        note(&mut log, "capabilities", serde_json::json!(capabilities))?;
        if app_id_text.ends_with(".i2cp") {
            i2cp_round_trip(&mut session, &mut log).await?;
        } else {
            sam_round_trip(&mut session, &mut log).await?;
        }
        note(
            &mut log,
            "complete",
            serde_json::json!({"scenario":"external-sdk"}),
        )?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    Ok(())
}

async fn sam_round_trip<R, W>(
    session: &mut Session<R, W>,
    log: &mut std::fs::File,
) -> Result<(), Box<dyn std::error::Error>>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let stream = session.open(AppService::Sam).await?;
    session
        .send(stream, b"HELLO VERSION MIN=3.1 MAX=3.1\r\n")
        .await?;
    let version = read_line(session, stream).await?;
    if !version.contains("RESULT=OK") || !version.contains("VERSION=3.1") {
        return Err("SAM HELLO refused".into());
    }
    note(log, "sam-version", serde_json::json!({"reply":version}))?;
    let id = "external-sdk-session";
    session
        .send(
            stream,
            format!("SESSION CREATE STYLE=STREAM ID={id} DESTINATION=TRANSIENT\r\n").as_bytes(),
        )
        .await?;
    let result = read_line(session, stream).await?;
    if !result.contains("RESULT=OK") {
        return Err("SAM SESSION CREATE refused".into());
    }
    note(
        log,
        "sam-session",
        serde_json::json!({"id":id,"result":result}),
    )?;
    session.close_stream(stream).await?;
    Ok(())
}

async fn i2cp_round_trip<R, W>(
    session: &mut Session<R, W>,
    log: &mut std::fs::File,
) -> Result<(), Box<dyn std::error::Error>>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let stream = session.open(AppService::I2cp).await?;
    let version = b"0.9.67";
    let mut request = vec![0x2a];
    let body_len = 1 + version.len();
    request.extend_from_slice(&(body_len as u32).to_be_bytes());
    request.push(32);
    request.push(version.len() as u8);
    request.extend_from_slice(version);
    session.send(stream, &request).await?;
    note(
        log,
        "i2cp-get-date",
        serde_json::json!({"bytes":request.len(),"stream_id":stream}),
    )?;
    let reply = read_i2cp_frame(session, stream).await?;
    if reply.get(4) != Some(&33) {
        return Err("I2CP GetDate returned an unexpected message".into());
    }
    note(
        log,
        "i2cp-set-date",
        serde_json::json!({"bytes":reply.len(),"stream_id":stream}),
    )?;
    session.close_stream(stream).await?;
    Ok(())
}

async fn read_line<R, W>(
    session: &mut Session<R, W>,
    stream: u32,
) -> Result<String, Box<dyn std::error::Error>>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut bytes = Vec::new();
    while !bytes.contains(&b'\n') {
        let frame = session.receive().await?;
        if frame.stream_id != stream {
            return Err("received data for another stream".into());
        }
        if bytes.len() + frame.payload.len() > 4096 {
            return Err("SAM line exceeded bound".into());
        }
        bytes.extend_from_slice(&frame.payload);
    }
    Ok(String::from_utf8_lossy(&bytes).trim_end().to_owned())
}

async fn read_i2cp_frame<R, W>(
    session: &mut Session<R, W>,
    stream: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut bytes = Vec::new();
    let mut expected = None;
    while expected.is_none_or(|length| bytes.len() < length) {
        let frame = session.receive().await?;
        if frame.stream_id != stream {
            return Err("received data for another stream".into());
        }
        if bytes.len() + frame.payload.len() > 65536 {
            return Err("I2CP frame exceeded bound".into());
        }
        bytes.extend_from_slice(&frame.payload);
        if bytes.len() >= 4 {
            expected = Some(u32::from_be_bytes(bytes[..4].try_into()?) as usize + 4);
        }
    }
    Ok(bytes)
}

fn note(log: &mut std::fs::File, step: &str, detail: serde_json::Value) -> std::io::Result<()> {
    serde_json::to_writer(&mut *log, &serde_json::json!({"step":step,"detail":detail}))?;
    log.write_all(b"\n")?;
    log.flush()
}

struct BlockingStdin(std::io::Stdin);
impl tokio::io::AsyncRead for BlockingStdin {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use std::io::Read;
        match self.0.read(buffer.initialize_unfilled()) {
            Ok(n) => {
                buffer.advance(n);
                std::task::Poll::Ready(Ok(()))
            }
            Err(error) => std::task::Poll::Ready(Err(error)),
        }
    }
}
struct BlockingStdout(std::io::Stdout);
impl tokio::io::AsyncWrite for BlockingStdout {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        use std::io::Write;
        std::task::Poll::Ready(self.0.write(bytes))
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use std::io::Write;
        std::task::Poll::Ready(self.0.flush())
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
