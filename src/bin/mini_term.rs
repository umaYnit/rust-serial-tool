use rust_serial_tool::{Colorize, Result, SerialPort, SerialTool};

pub struct MiniTerm {
    name_short: String,
    target_serial_name: String,
    target_serial: Option<SerialPort>,
}

impl MiniTerm {
    pub fn initialize(target_serial_name: String) -> Self {
        Self {
            name_short: "MT".to_string(),
            target_serial_name,
            target_serial: None,
        }
    }
}

impl SerialTool for MiniTerm {
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


    fn exec(&mut self) -> Result<()> {
        self.open_serial();
        self.terminal()
    }
}


fn main() {
    let target_serial_name: String = std::env::args().nth(1).expect("expect arg [serial_name]");

    println!("{}", "Miniterm 1.0\n".cyan());
    let mut mini_push = MiniTerm::initialize(target_serial_name);
    mini_push.run();
}

