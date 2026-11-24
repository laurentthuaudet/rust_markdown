//! Single application instance.
//!
//! The first process owns a named pipe. Later launches (double-click on a
//! `.md` file, "Open with…", the Start menu) hand their files to it through
//! the pipe and exit, so documents open as tabs of one window instead of in
//! new windows.

use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericNamespaced, Listener, ListenerOptions, Name, Stream};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// Files sent by another launch. An empty list only asks to show the window.
pub type Request = Vec<PathBuf>;

/// Receiving end handed to the interface (taken once, by the root component).
#[derive(Clone, Default)]
pub struct Requests(Arc<Mutex<Option<UnboundedReceiver<Request>>>>);

impl Requests {
    pub fn take(&self) -> Option<UnboundedReceiver<Request>> {
        self.0.lock().ok()?.take()
    }
}

/// Outcome of [`claim`].
pub enum Instance {
    /// This process is the application: listen for later launches.
    Primary(Requests),
    /// The files were handed to the running application: exit.
    Forwarded,
}

/// Pipe name, per Windows user so that sessions of different users do not meet.
fn pipe_name(app: &str) -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    format!("{app}-{user}.pipe")
}

/// Becomes the primary instance, or forwards `files` to the existing one.
///
/// Explorer starts one process per file when several are opened at once, so
/// two launches may race: whoever creates the pipe first wins, the others
/// retry connecting while it starts listening.
pub fn claim(app: &str, files: &[PathBuf]) -> Instance {
    let name = pipe_name(app);
    for _ in 0..20 {
        let Ok(ns_name) = name.as_str().to_ns_name::<GenericNamespaced>() else {
            break;
        };
        match ListenerOptions::new().name(ns_name.clone()).create_sync() {
            Ok(listener) => return Instance::Primary(serve(listener)),
            Err(_) => {
                if forward(ns_name, files).is_ok() {
                    return Instance::Forwarded;
                }
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    // No pipe available: run standalone rather than not at all.
    Instance::Primary(Requests::default())
}

/// Sends one request: one UTF-8 path per line, closed by an empty line.
fn forward(name: Name<'_>, files: &[PathBuf]) -> io::Result<()> {
    let mut stream = Stream::connect(name)?;
    let mut message = String::new();
    for file in files {
        // Absolute, since the receiving process has another working directory.
        let file = std::path::absolute(file).unwrap_or_else(|_| file.clone());
        if let Some(text) = file.to_str() {
            message.push_str(text);
            message.push('\n');
        }
    }
    message.push('\n');
    stream.write_all(message.as_bytes())?;
    stream.flush()
}

fn serve(listener: Listener) -> Requests {
    let (sender, receiver) = unbounded_channel();
    thread::spawn(move || {
        for connection in listener.incoming().flatten() {
            // A misbehaving client must not stop the listener.
            let _ = receive(connection, &sender);
        }
    });
    Requests(Arc::new(Mutex::new(Some(receiver))))
}

fn receive(connection: Stream, sender: &UnboundedSender<Request>) -> io::Result<()> {
    let mut files = Vec::new();
    for line in BufReader::new(connection).lines() {
        let line = line?;
        if line.is_empty() {
            break;
        }
        files.push(PathBuf::from(line));
    }
    let _ = sender.send(files);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_launch_forwards_its_files() {
        let app = format!("rmd-test-{}", std::process::id());
        let Instance::Primary(requests) = claim(&app, &[]) else {
            panic!("the first launch must be primary");
        };
        let mut receiver = requests.take().unwrap();

        let files = [
            PathBuf::from(r"C:\notes\a.md"),
            PathBuf::from(r"C:\notes\b é.md"),
        ];
        assert!(matches!(claim(&app, &files), Instance::Forwarded));
        assert!(matches!(claim(&app, &[]), Instance::Forwarded));

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime.block_on(async {
            assert_eq!(receiver.recv().await.unwrap(), files.to_vec());
            assert_eq!(receiver.recv().await.unwrap(), Vec::<PathBuf>::new());
        });
    }
}
