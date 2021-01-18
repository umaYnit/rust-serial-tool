use std::{fs::File, io::{Read, Write}, sync::{Arc, atomic::{AtomicBool, Ordering}}};

use rust_serial_tool::{Colorize, create_pb, ErrorKind, ReadSerial, Result, SerialPort, SerialTool, sleep, timeout};

pub struct MiniPush {
    name_short: String,
    binary_image_path: String,
    target_serial_name: String,
    target_serial: Option<SerialPort>,
}


impl MiniPush {
    pub fn initialize(target_serial_name: String, binary_image_path: String) -> Self {
        Self {
            name_short: "MP".to_string(),
            binary_image_path,
            target_serial_name,
            target_serial: None,
        }
    }

    fn wait_for_binary_request(&mut self) -> Result<()> {
        println!("[{}] 🔌 Please power the target now", self.name_short);
        let serial = self.target_serial().ok_or(ErrorKind::NoneError("serial"))?;

        let f = move |flag: Arc<AtomicBool>| -> Result<()> {
            let mut received = [0; 4096];

            let mut n = serial.read_serial(&mut received).map_err(|_| ErrorKind::ConnectionError)?;
            let mut count = 0;
            while flag.load(Ordering::Relaxed) {
                for &c in received[..n].iter() {
                    if c == 0x03 {
                        count += 1;
                        if count == 3 {
                            return Ok(());
                        }
                    } else {
                        count = 0;
                        print!("{}", c as char);
                    }
                }
                n = serial.read_serial(&mut received).map_err(|_| ErrorKind::ConnectionError)?;
            }
            Ok(())
        };

        timeout(f, 10)
    }

    fn load_binary(&mut self) -> Result<(File, u64)> {
        let file = std::fs::File::open(&self.binary_image_path)?;
        let binary_size = file.metadata()?.len();

        Ok((file, binary_size))
    }
    fn send_size(&mut self, binary_size: u64) -> Result<()> {
        let serial = self.target_serial().ok_or(ErrorKind::NoneError("serial"))?;
        // pi just read 4 byt, and u64 will convert to [u8;8]
        serial.write_all(&(binary_size as u32).to_le_bytes())?;

        let mut received = [0; 2];
        serial.read_serial_exact(&mut received).map_err(|_| ErrorKind::ProtocolError)?;
        if received != "OK".as_bytes() { Err(ErrorKind::ProtocolError) } else { Ok(()) }
    }


    fn send_binary(&mut self, (mut binary_image, binary_size): (File, u64)) -> Result<()> {
        let name_short = self.name_short();
        let mut pb = create_pb(name_short, binary_size);

        let serial = self.target_serial().ok_or(ErrorKind::NoneError("serial"))?;

        let mut progress = 0;

        while progress < pb.total {
            let mut chunk = Vec::with_capacity(512);
            let n = std::io::Read::by_ref(&mut binary_image).take(512).read_to_end(&mut chunk)?;
            serial.write_all(&chunk[..n])?;
            progress = pb.add(n as u64);
        }
        pb.finish();
        println!("[{}] send finish!", self.name_short());
        Ok(())
    }
}


impl SerialTool for MiniPush {
    fn target_serial_name(&self) -> &str {
        &self.target_serial_name
    }

    fn name_short(&self) -> &str {
        &self.name_short
    }

    fn target_serial(&mut self) -> Option<&mut SerialPort> {
        self.target_serial.as_mut()
    }

    fn set_target_serial(&mut self, serialport: SerialPort) {
        self.target_serial = Some(serialport);
    }

    fn handle_reconnect(&mut self) {
        self.connection_reset();
        println!("\n[{}] ⚡ {} {}",
                 self.name_short(),
                 "Connection or protocol Error: ".red(),
                 "Remove power and USB serial. Reinsert serial first, then power".red());

        while !self.serial_connected() { sleep(1) }
    }

    fn exec(&mut self) -> Result<()> {
        self.open_serial();
        self.wait_for_binary_request()?;

        let result = self.load_binary()?;
        self.send_size(result.1)?;
        self.send_binary(result)?;
        self.terminal()
    }
}

fn main() {
    let target_serial_name: String = std::env::args().nth(1).expect("expect arg [serial_name]");
    let binary_image_path: String = std::env::args().nth(2).expect("expect arg [image_path]");


    println!("{}", "Minipush 1.0\n".cyan());
    let mut mini_push = MiniPush::initialize(target_serial_name, binary_image_path);
    mini_push.run();
}
