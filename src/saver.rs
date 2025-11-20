use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

pub enum SaverMessage {
    Save(String),
    Open(PathBuf),
}

pub enum SaverResponse {
    Loaded(String),
}

pub struct Saver {
    receiver: Receiver<SaverMessage>,
    response_sender: Sender<SaverResponse>,
}

impl Saver {
    pub fn new(receiver: Receiver<SaverMessage>, response_sender: Sender<SaverResponse>) -> Self {
        Self {
            receiver,
            response_sender,
        }
    }

    pub fn run(&self) {
        // Ensure data directory exists
        let data_dir = Path::new("data");
        if !data_dir.exists() {
            let _ = fs::create_dir(data_dir);
        }

        while let Ok(message) = self.receiver.recv() {
            match message {
                SaverMessage::Save(content) => {
                    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
                    let filename = format!("{}.txt", timestamp);
                    let file_path = data_dir.join(filename);
                    if let Err(e) = fs::write(&file_path, content) {
                        eprintln!("Failed to save file: {}", e);
                    } else {
                        println!("File saved successfully to {:?}", file_path);
                    }
                }
                SaverMessage::Open(path) => {
                    match fs::read_to_string(&path) {
                        Ok(content) => {
                            if let Err(e) = self
                                .response_sender
                                .send(SaverResponse::Loaded(content))
                            {
                                eprintln!("Failed to send loaded content: {}", e);
                            }
                        }
                        Err(e) => eprintln!("Failed to read file {:?}: {}", path, e),
                    }
                }
            }
        }
    }
}

pub fn spawn_saver() -> (Sender<SaverMessage>, Receiver<SaverResponse>) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let (response_sender, response_receiver) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let saver = Saver::new(receiver, response_sender);
        saver.run();
    });
    (sender, response_receiver)
}
