use std::{
    fs::File,
    io::{self, Write},
};

struct DownloadFile {
    name: String,
    dir: String,
    file: File,
}

impl DownloadFile {
    pub fn new(name: String, dir: String) -> Result<Self, io::Error> {
        let file = File::create([name.clone(), dir.clone()].concat())?;
        Ok(DownloadFile { name, dir, file })
    }

    pub fn write(&mut self, buf: Vec<u8>) -> Result<usize, io::Error> {
        self.file.write(&buf)
    }
}
