use log::{debug, info};
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::{stdin, stdout, BufReader};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub(crate) fn main() -> Result<()> {
    env_logger::init();
    debug!("inited");

    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();

    runtime.block_on(try_main())
}

async fn try_main() -> Result<()> {
    let stdin = BufReader::new(stdin());
    let mut stdout = stdout();
    let mut lines_from_stdin = stdin.lines();
    while let Some(line) = lines_from_stdin.next_line().await? {
        info!("Received {}", line);
        stdout.write_all(line.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
    }
    Ok(())
}
