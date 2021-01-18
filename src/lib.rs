#![feature(type_alias_impl_trait)]


use std::{io, io::{Read, Stdout, stdout, Write}, panic, thread};
use std::path::Path;
use std::process::exit;
use std::sync::{Arc, atomic::{AtomicBool, AtomicU8, Ordering}, Condvar, Mutex};
use std::time::Duration;

pub use crossterm::{style::Colorize, terminal::{disable_raw_mode, enable_raw_mode}};

#[cfg(unix)]
pub type SerialPort = serialport::TTYPort;
#[cfg(windows)]
pub type SerialPort = serialport::COMPort;

pub const SERIAL_BAUD: u32 = 921_600;

pub trait SerialTool {
    fn target_serial_name(&self) -> &str;
    fn name_short(&self) -> &str;
    fn target_serial(&mut self) -> Option<&mut SerialPort>;
    fn set_target_serial(&mut self, serialport: SerialPort);

    fn serial_connected(&self) -> bool {
        if cfg!(unix) {
            Path::new(self.target_serial_name()).exists()
        } else if cfg!(windows) {
            serialport::available_ports()
                .expect("find ports error")
                .iter()
                .any(|port| {
                    port.port_name == self.target_serial_name()
                })
        } else {
            panic!("unsupported system")
        }
    }

    fn wait_for_serial(&self) {
        if self.serial_connected() { return; }
        println!("[{}] ⏳ Waiting for {}", self.name_short(), self.target_serial_name());

        while !self.serial_connected() { sleep(1); }
    }

    fn open_serial(&mut self) {
        self.wait_for_serial();
        match serialport::new(self.target_serial_name(), SERIAL_BAUD)
            .timeout(Duration::from_millis(1))
            .open_native() {
            Ok(target_serial) => {
                println!("[{}] ✅ Connected", self.name_short());
                self.set_target_serial(target_serial);
            }
            Err(e) => {
                print!("[{}] 🚫 {}", self.name_short(), e);
                exit(-1);
            }
        };
    }

    fn terminal(&mut self) -> Result<()> {
        let port = self.target_serial().ok_or(ErrorKind::NoneError("serial"))?;

        let mut serial_port = port.try_clone_native()?;


        enable_raw_mode().unwrap();
        // 0: ok, no error; 1: connect error; 2: ctrl c
        let has_error = Arc::new(AtomicU8::new(0));
        let has_error_clone = has_error.clone();

        thread::spawn(move || {
            let mut serial_buf = [0; 256];
            while is_ok(&*has_error_clone) {
                match serial_port.read_serial(&mut serial_buf) {
                    Ok(t) => {
                        String::from_utf8_lossy(&serial_buf[..t]).chars().for_each(|c| {
                            if c == '\n' {
                                print!("\r");
                            }
                            print!("{}", c);
                        });
                        stdout().flush().unwrap();
                    }
                    Err(e) => {
                        print!("\r\nread_serial error {:?}", e);
                        has_error_clone.store(1, Ordering::Relaxed);
                        break;
                    }
                }
            }
        });

        let mut console_buf = [0; 256];

        while is_ok(&*has_error) {
            let len = io::stdin().read(&mut console_buf)?;

            if console_buf.contains(&03) { has_error.store(2, Ordering::Relaxed); }

            port.write_all(&mut console_buf[..len]).map_err(|_| ErrorKind::ConnectionError)?;
        }

        if has_error.load(Ordering::Relaxed) == 1 { Err(ErrorKind::ConnectionError) } else { Ok(()) }
    }

    fn connection_reset(&mut self) {
        self.target_serial().take();
        disable_raw_mode().unwrap();
    }

    fn handle_reconnect(&mut self) {
        self.connection_reset();
        println!("\n[{}] ⚡ {}", self.name_short(), "Connection Error: Reinsert the USB serial again".red());
    }

    fn handle_unexpected(&mut self, error: ErrorKind) {
        self.connection_reset();
        println!("\n[{}] ⚡ {}", self.name_short(), format!("Unexpected Error: #{:?}", error).red());
    }

    fn exec(&mut self) -> Result<()>;
    fn run(&mut self) {
        panic::set_hook(Box::new(|info| {
            disable_raw_mode().unwrap();
            println!("{}", info);
        }));
        loop {
            if let Err(e) = self.exec() {
                match e {
                    ErrorKind::ConnectionError |
                    ErrorKind::ProtocolError |
                    ErrorKind::TimeoutError => {
                        self.handle_reconnect();
                    }
                    _ => {
                        self.handle_unexpected(e);
                        break;
                    }
                }
            } else { break; }
        }
        self.connection_reset();
        println!("\n[{}] Bye 👋", self.name_short());
    }
}

fn is_ok(flag: &AtomicU8) -> bool {
    flag.load(Ordering::Relaxed) == 0
}


pub fn sleep(sec: u64) {
    thread::sleep(Duration::from_secs(sec));
}

pub fn create_pb(name_short: &str, total: u64) -> pbr::ProgressBar<Stdout> {
    let mut pb = pbr::ProgressBar::new(total);
    pb.set_units(pbr::Units::Bytes);
    pb.set_width(Some(92));
    pb.show_counter = false;
    pb.message(&format!("[{}] ⏩ Pushing 6 KiB", name_short));
    pb.format(" =🦀- ");
    pb
}

pub fn timeout<F>(f: F, sec: u64) -> Result<()>
    where
        F: FnOnce(Arc<AtomicBool>) -> Result<()>,
        F: Send {
    let flag = Arc::new(AtomicBool::new(true));

    let pair = Arc::new((Mutex::new(true), Condvar::new()));
    let pair2 = Arc::clone(&pair);
    let flag_clone = flag.clone();

    thread::spawn(move || {
        let (lock, cvar) = &*pair2;
        let mut ok = lock.lock().unwrap();
        loop {
            let res = cvar.wait_timeout(ok, Duration::from_secs(sec)).unwrap();
            ok = res.0;

            flag_clone.store(false, Ordering::Relaxed);
            break;
        }
    });

    let (_, cvar) = &*pair;
    f(flag.clone())?;
    cvar.notify_one();


    if flag.load(Ordering::Relaxed) { Ok(()) } else { Err(ErrorKind::TimeoutError) }
}

pub trait ReadSerial {
    fn read_serial(&mut self, buf: &mut [u8]) -> Result<usize>;
    fn read_serial_exact(&mut self, buf: &mut [u8]) -> Result<()>;
}

impl ReadSerial for SerialPort {
    fn read_serial(&mut self, buf: &mut [u8]) -> Result<usize> {
        match self.read(buf) {
            Ok(t) => Ok(t),
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => Ok(0),
            Err(ref e) if match e.raw_os_error() {
                Some(22) | Some(1167) => true,
                _ => false
            } => Err(ErrorKind::ConnectionError),
            Err(e) => Err(ErrorKind::IoError(e))
        }
    }

    fn read_serial_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.read_serial(buf) {
                Ok(n) => {
                    let tmp = buf;
                    buf = &mut tmp[n..];
                }
                Err(ErrorKind::IoError(ref e)) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() { Err(ErrorKind::ConnectionError) } else { Ok(()) }
    }
}


pub type Result<T> = std::result::Result<T, ErrorKind>;


#[derive(Debug)]
#[non_exhaustive]
pub enum ErrorKind {
    ConnectionError,
    ProtocolError,
    TimeoutError,
    NoneError(&'static str),
    SerialError(serialport::Error),
    IoError(io::Error),
}

impl_from!(io::Error, ErrorKind::IoError);
impl_from!(serialport::Error, ErrorKind::SerialError);


#[macro_export]
macro_rules! impl_from {
    ($from:path, $to:expr) => {
        impl From<$from> for ErrorKind {
            fn from(e: $from) -> Self {
                $to(e)
            }
        }
    };
}