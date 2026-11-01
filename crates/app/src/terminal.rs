use std::io::{self, BufRead};

pub enum Command {
    Stop,
    Kill,
    Status,
}

pub fn spawn(commands: async_channel::Sender<Command>) -> io::Result<()> {
    std::thread::Builder::new()
        .name("lucent-terminal".into())
        .spawn(move || {
            tracing::info!("type stop, kill or status, then press Enter");
            for line in io::stdin().lock().lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(error) => {
                        tracing::warn!(%error, "couldn't read terminal input");
                        break;
                    }
                };
                match line.trim() {
                    "stop" => {
                        if commands.send_blocking(Command::Stop).is_err() {
                            break;
                        }
                    }
                    "kill" => {
                        if commands.send_blocking(Command::Kill).is_err() {
                            break;
                        }
                    }
                    "status" => {
                        if commands.send_blocking(Command::Status).is_err() {
                            break;
                        }
                    }
                    "" => {}
                    line => tracing::warn!("unknown command {line:?}; type stop, kill or status"),
                }
            }
        })?;
    Ok(())
}
