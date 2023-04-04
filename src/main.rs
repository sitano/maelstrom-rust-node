use async_std::io::stdin;
use async_std::io::stdout;
use async_std::io::BufReader;
use async_std::prelude::*;
use async_std::task;
use futures::StreamExt;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub(crate) fn main() -> Result<()> {
    task::block_on(try_main())
}

async fn try_main() -> Result<()> {
    let stdin = BufReader::new(stdin());
    let mut stdout = stdout();
    let mut lines_from_stdin = futures::StreamExt::fuse(stdin.lines());
    loop {
        match lines_from_stdin.next().await {
            Some(res) => {
                let line = res?;
                stdout.write_all(line.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
            }
            None => break,
        }
    }
    Ok(())
}
