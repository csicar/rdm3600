use std::error::Error;

use rppal::uart::{Uart, Parity};


fn main() -> Result<(), Box<dyn Error>> {
    println!("Setup UART...");
    let uart = Uart::new(9_600, Parity::None, 8, 1)?;

    println!("Install RDM3600 Driver...");
    let mut rdm3600 = rdm3600_rs::Rdm6300::new(uart);

    println!("Waiting for a Scan...");
    loop {
        match rdm3600.read() {
            Ok(rfid) =>{
                println!("Received RFID: {:?}", rfid);

            },
            Err(nb::Error::WouldBlock) => {
                // Just wait for next round
            },
            Err(nb::Error::Other(err)) => {
                eprintln!("error: {:?}", err)
            }
        }
    }
}
