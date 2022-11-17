#![no_std]

use embedded_hal::serial::{self, Read};

// Frame of 14 bytes
// Head : 1byte (==2)
// Data:  10 byte (ascii encoded hex)
// Checksum: 2 byte
// Tail: 1 byte  (==3)
pub enum State {
    ReadHead,
    ReadBody,
    ReadTail,
}

#[derive(Debug, Eq, PartialEq)]
pub struct RfidTag {
    pub id: [u8; 5],
}

#[derive(Debug)]
pub enum DecodeError {
    InvalidHead,
    InvalidTail,
    InvalidChecksum,
    InvalidData,
}

#[derive(Debug)]
pub enum Error<E> {
    SerialError(E),
    DecodeError(DecodeError),
}

impl<E> From<E> for Error<E> {
    fn from(err: E) -> Self {
        Error::SerialError(err)
    }
}

pub struct Rdm6300<R: Read<u8>> {
    serial: R,
    state: State,
    buffer: [u8; 12],
    offset: usize,
}

impl<R: Read<u8>> Rdm6300<R> {
    pub fn new(serial: R) -> Self {
        Rdm6300 {
            serial,
            state: State::ReadHead,
            buffer: [0; 12],
            offset: 0,
        }
    }
    fn read_byte(dev: &mut R) -> nb::Result<u8, Error<R::Error>> {
        dev.read()
            .map_err(|e: nb::Error<R::Error>| e.map(Error::SerialError))
    }

    fn read_bytes<const L: usize>(&mut self) -> nb::Result<(), Error<R::Error>> {
        loop {
            if self.offset < L {
                let byte = Self::read_byte(&mut self.serial)?;
                self.buffer[self.offset] = byte;
                self.offset += 1;
            } else {
                return Ok(());
            }
        }
    }

    /// Reset State Machine to prepare for a new package
    pub fn reset(&mut self) {
        self.offset = 0;
        self.state = State::ReadHead;
    }

    /// Reads a single RFID-Tag.
    /// Returns `WouldBlock` if not enough data is available on the serial interface
    /// Returns `Error` if reading the RFID-Tag failed
    pub fn read(&mut self) -> nb::Result<RfidTag, Error<R::Error>> {
        loop {
            match self.state {
                State::ReadHead => {
                    let byte = Self::read_byte(&mut self.serial)?;
                    if byte == 0x02 {
                        self.state = State::ReadBody;
                    } else {
                        return Err(nb::Error::Other(Error::DecodeError(
                            DecodeError::InvalidHead,
                        )));
                    }
                }
                State::ReadBody => {
                    self.read_bytes::<12>()?;
                    self.state = State::ReadTail
                }
                State::ReadTail => {
                    let byte = Self::read_byte(&mut self.serial)?;
                    if byte == 0x03 {
                        self.reset()
                    } else {
                        self.reset();
                        return Err(nb::Error::Other(Error::DecodeError(
                            DecodeError::InvalidTail,
                        )));
                    }
                    return decode(&self.buffer)
                        .map_err(Error::DecodeError)
                        .map_err(nb::Error::Other);
                }
            }
        }
    }
}

fn ascii_encoded_to_value(ascii: u8) -> Option<u8> {
    let ascii_char = ascii as char;
    if let Some(value) = ascii_char.to_digit(16) {
        Some(value as u8)
    } else {
        None
    }
}

fn decode(data: &[u8; 12]) -> Result<RfidTag, DecodeError> {
    let mut decoded_data = [0u8; 5];
    for i in 0..decoded_data.len() {
        decoded_data[i] = ascii_encoded_to_value(data[i * 2]).ok_or(DecodeError::InvalidData)?
            * 2u8.pow(4)
            + ascii_encoded_to_value(data[i * 2 + 1]).ok_or(DecodeError::InvalidData)?;
    }

    let decoded_checksum = ascii_encoded_to_value(data[10]).ok_or(DecodeError::InvalidData)?
        * 2u8.pow(4)
        + ascii_encoded_to_value(data[11]).ok_or(DecodeError::InvalidData)?;

    let mut expected_checksum = 0u8;
    for byte in decoded_data {
        expected_checksum ^= byte;
    }
    if expected_checksum == decoded_checksum {
        Ok(RfidTag { id: decoded_data })
    } else {
        Err(DecodeError::InvalidChecksum)
    }
}

#[test]
fn ascii_decode() {
    let asd = ascii_encoded_to_value(0x43).unwrap();
    assert_eq!(asd, 12);
    let asd = ascii_encoded_to_value(0x31).unwrap();
    assert_eq!(asd, 1);
    let asd = ascii_encoded_to_value('0' as u8).unwrap();
    assert_eq!(asd, 0);
    let asd = ascii_encoded_to_value('A' as u8).unwrap();
    assert_eq!(asd, 10);
}

#[test]
fn example() {
    let asd = decode(&[
        0x31, 0x34, 0x30, 0x30, 0x38, 0x45, 0x43, 0x37, 0x39, 0x33, // CS
        0x43, 0x45,
    ])
    .unwrap();
    assert_eq!(asd.id, [0x14, 0x00, 0x8E, 0xC7, 0x93])
}

#[should_panic]
#[test]
fn example_invalid_checksum() {
    let asd = decode(&[
        0x31, 0x34, 0x30, 0x30, 0x38, 0x45, 0x43, 0x37, 0x39, 0x33, //CS
        0x43, 0x46,
    ])
    .unwrap();
}

#[cfg(test)]
mod test {
    use embedded_hal_mock::{
        serial::{Mock as SerialMock, Transaction as SerialTransaction},
        MockError,
    };
    use nb::block;

    use crate::{DecodeError, Error, Rdm6300, RfidTag};

    #[test]
    fn serial_happy() {
        let expectations = [
            SerialTransaction::read(0x02_u8),
            SerialTransaction::read_many(b"14008EC793CE"),
            SerialTransaction::read(0x03_u8),
        ];
        let serial = SerialMock::new(&expectations);
        let mut rdm = Rdm6300::new(serial);
        let rfid = rdm.read().unwrap();
        assert_eq!(
            rfid,
            RfidTag {
                id: [0x14, 0x00, 0x8e, 0xc7, 0x93]
            }
        );
    }

    #[test]
    fn serial_wrong_start_recover() {
        let expectations = [
            SerialTransaction::read(0x01_u8),
            SerialTransaction::read(0x02_u8),
            SerialTransaction::read_many(b"14008EC793CE"),
            SerialTransaction::read(0x03_u8),
        ];
        let serial = SerialMock::new(&expectations);
        let mut rdm = Rdm6300::new(serial);
        rdm.read().expect_err("invalid start");
        let rfid = rdm.read().unwrap();
        assert_eq!(
            rfid,
            RfidTag {
                id: [0x14, 0x00, 0x8e, 0xc7, 0x93]
            }
        );
    }

    #[test]
    fn serial_wrong_checksum_fail() {
        let expectations = [
            SerialTransaction::read(0x02_u8),
            SerialTransaction::read_many(b"14008EC793CC"),
            SerialTransaction::read(0x03_u8),
        ];
        let serial = SerialMock::new(&expectations);
        let mut rdm = Rdm6300::new(serial);
        let err = rdm.read().expect_err("invalid checksum");
        match err {
            nb::Error::Other(Error::DecodeError(DecodeError::InvalidChecksum)) => (),
            _ => panic!("wrong error"),
        }
    }

    #[test]
    fn serial_block_recover() {
        let expectations = [
            SerialTransaction::read(0x02_u8),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_many(b"14008EC"),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_many(b"793CE"),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read(0x03_u8),
        ];
        let serial = SerialMock::new(&expectations);
        let mut rdm = Rdm6300::new(serial);
        let rfid = block!(rdm.read()).unwrap();
        assert_eq!(
            rfid,
            RfidTag {
                id: [0x14, 0x00, 0x8e, 0xc7, 0x93]
            }
        );
    }

    #[test]
    fn serial_block_recover_2() {
        let expectations = [
            // First Scan
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read(0x02_u8),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_many(b"14008EC"),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_many(b"793CE"),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read(0x03_u8),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            // Second Scan
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read(0x02_u8),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_many(b"14008EC"),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_many(b"793CE"),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read_error(nb::Error::WouldBlock),
            SerialTransaction::read(0x03_u8),
            SerialTransaction::read_error(nb::Error::WouldBlock),
        ];
        let serial = SerialMock::new(&expectations);
        let mut rdm = Rdm6300::new(serial);
        let rfid = block!(rdm.read()).unwrap();
        let rfid = block!(rdm.read()).unwrap();
        assert_eq!(
            rfid,
            RfidTag {
                id: [0x14, 0x00, 0x8e, 0xc7, 0x93]
            }
        );
    }
}
