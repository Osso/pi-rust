//! Separable Doom wrapper. A host supplies the waived external module; this
//! file does not load/evaluate the generated Emscripten JavaScript asset.
use std::{fmt, path::PathBuf};

use pi_js_compat::node_fs::{NodeFsError, NodeReadApi, node_read_error_record};

#[derive(Debug)]
pub enum DoomInitError {
    Wad(NodeFsError),
    Module(String),
}

impl fmt::Display for DoomInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wad(error) => f.write_str(&error.message),
            Self::Module(message) => f.write_str(message),
        }
    }
}

impl From<String> for DoomInitError {
    fn from(message: String) -> Self {
        Self::Module(message)
    }
}

/// Native fixture/host boundary for the Emscripten operations used by this example.
/// Not a new core extension API or a WASM interpreter.
pub trait DoomModule: Send {
    fn create_path(&mut self, parent: &str, path: &str) -> Result<(), String>;
    fn create_data_file(&mut self, parent: &str, name: &str, bytes: &[u8]) -> Result<(), String>;
    fn malloc(&mut self, size: u32) -> Result<u32, String>;
    fn set_value(&mut self, ptr: u32, value: u32, kind: &str) -> Result<(), String>;
    fn create(&mut self, argc: u32, argv: u32) -> Result<(), String>;
    fn free(&mut self, ptr: u32) -> Result<(), String>;
    fn frame_buffer(&self) -> Result<u32, String>;
    fn width(&self) -> Result<usize, String>;
    fn height(&self) -> Result<usize, String>;
    fn get_i32(&self, ptr: u32) -> Result<i32, String>;
    fn tick(&mut self) -> Result<(), String>;
    fn push_key(&mut self, pressed: bool, key: u32) -> Result<(), String>;
}

pub struct DoomEngine {
    module: Option<Box<dyn DoomModule>>,
    frame_buffer_ptr: u32,
    initialized: bool,
    wad_path: PathBuf,
    width: usize,
    height: usize,
}

impl DoomEngine {
    pub fn new(wad_path: PathBuf) -> Self {
        Self {
            module: None,
            frame_buffer_ptr: 0,
            initialized: false,
            wad_path,
            width: 640,
            height: 400,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Caller supplies the external module instead of loading `doom/build/doom.js`.
    /// Pre-run filesystem setup and all subsequent ABI calls retain source order.
    pub fn init_with_module(&mut self, mut module: Box<dyn DoomModule>) -> Result<(), DoomInitError> {
        let wad = std::fs::read(&self.wad_path).map_err(|error| {
            DoomInitError::Wad(node_read_error_record(
                &error,
                &self.wad_path.to_string_lossy(),
                NodeReadApi::ReadFileSync,
            ))
        })?;
        module.create_path("/", "doom")?;
        module.create_data_file("/doom", "doom1.wad", &wad)?;
        self.module = Some(module);
        let module = self.module.as_mut().unwrap();
        init_doom(module.as_mut())?;
        self.frame_buffer_ptr = module.frame_buffer()?;
        self.width = module.width()?;
        self.height = module.height()?;
        self.initialized = true;
        Ok(())
    }

    pub fn tick(&mut self) -> Result<(), String> {
        if !self.initialized {
            return Ok(());
        }
        if let Some(module) = &mut self.module {
            module.tick()?;
        }
        Ok(())
    }

    pub fn get_frame_rgba(&self) -> Result<Vec<u8>, String> {
        let mut buffer = vec![0; self.width * self.height * 4];
        let Some(module) = self.module.as_ref().filter(|_| self.initialized) else {
            return Ok(buffer);
        };
        for (i, pixel) in buffer.chunks_exact_mut(4).enumerate() {
            let argb = module.get_i32(self.frame_buffer_ptr + (i as u32) * 4)?;
            pixel.copy_from_slice(&[
                ((argb >> 16) & 0xff) as u8,
                ((argb >> 8) & 0xff) as u8,
                (argb & 0xff) as u8,
                255,
            ]);
        }
        Ok(buffer)
    }

    pub fn push_key(&mut self, pressed: bool, key: u32) -> Result<(), String> {
        if !self.initialized {
            return Ok(());
        }
        if let Some(module) = &mut self.module {
            module.push_key(pressed, key)?;
        }
        Ok(())
    }
}

fn init_doom(module: &mut dyn DoomModule) -> Result<(), String> {
    let args = ["doom", "-iwad", "/doom/doom1.wad"];
    let mut pointers = Vec::new();
    for arg in args {
        let pointer = module.malloc(arg.len() as u32 + 1)?;
        for (i, byte) in arg.bytes().enumerate() {
            module.set_value(pointer + i as u32, u32::from(byte), "i8")?;
        }
        module.set_value(pointer + arg.len() as u32, 0, "i8")?;
        pointers.push(pointer);
    }
    let argv = module.malloc(pointers.len() as u32 * 4)?;
    for (i, pointer) in pointers.iter().enumerate() {
        module.set_value(argv + i as u32 * 4, *pointer, "i32")?;
    }
    module.create(args.len() as u32, argv)?;
    // Source does not free allocations on create failure; preserve that ordering.
    for pointer in pointers {
        module.free(pointer)?;
    }
    module.free(argv)
}
