use core::ops::{Deref, DerefMut};

use embedded_hal::serial::{Read, Write};

use crate::memory_mapped::MemoryMapped;

const SIODATA8: MemoryMapped<u16> = unsafe { MemoryMapped::new(0x0400_012A) };
const SIOCNT: MemoryMapped<u16> = unsafe { MemoryMapped::new(0x0400_0128) };
const RCNT: MemoryMapped<u16> = unsafe { MemoryMapped::new(0x0400_0134) };

#[derive(Debug)]
pub enum LinkPortError {
    GbaErrorBit,
}

pub struct LinkPortUart;

impl LinkPortUart {
    pub fn init(rate: BaudRate, with_interrupts: bool, clear_to_send: bool) -> Self {
        RCNT.set(0x0);
        SIOCNT.set(0x0);
        let reg: u16 = SioControlReg::default_uart()
            .with_baud(rate)
            .with_interrupts(with_interrupts)
            .with_cts(clear_to_send)
            .into();
        SIOCNT.set(reg);
        Self
    }
}

impl Read<u8> for LinkPortUart {
    type Error = LinkPortError;

    fn read(&mut self) -> Result<u8, nb::Error<LinkPortError>> {
        match SioControlReg::from(SIOCNT.get()) {
            v if *v.error => Err(nb::Error::Other(LinkPortError::GbaErrorBit)),
            v if *v.recv_empty => Err(nb::Error::WouldBlock),
            _ => Ok((SIODATA8.get() & 0xFF) as u8),
        }
    }
}

impl Write<u8> for LinkPortUart {
    type Error = LinkPortError;

    fn write(&mut self, word: u8) -> nb::Result<(), Self::Error> {
        match self.flush() {
            Ok(_) => {
                SIODATA8.set(word as u16);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    fn flush(&mut self) -> nb::Result<(), Self::Error> {
        match SioControlReg::from(SIOCNT.get()) {
            v if *v.error => Err(nb::Error::Other(LinkPortError::GbaErrorBit)),
            v if *v.send_full => Err(nb::Error::WouldBlock),
            _ => Ok(()),
        }
    }
}

pub enum BaudRate {
    B9600 = 0b00,
    B38400 = 0b01,
    B57600 = 0b10,
    B115200 = 0b11,
}

impl From<u16> for BaudRate {
    fn from(value: u16) -> Self {
        match value {
            0b00 => Self::B9600,
            0b01 => Self::B38400,
            0b10 => Self::B57600,
            0b11 => Self::B115200,
            _ => panic!("passed invalid value"),
        }
    }
}

pub enum SioMode {
    Normal8bit = 0b00,
    Multiplayer = 0b01,
    Normal32bit = 0b10,
    Uart = 0b11,
}

impl From<u16> for SioMode {
    fn from(value: u16) -> Self {
        match value {
            0b00 => Self::Normal8bit,
            0b01 => Self::Multiplayer,
            0b10 => Self::Normal32bit,
            0b11 => Self::Uart,
            _ => panic!("passed invalid value"),
        }
    }
}

struct SioControlReg {
    baud_rate: BaudRate,       // 0-1
    flow_control: BoolField,   // 2
    parity_odd: BoolField,     // 3
    send_full: BoolField,      // 4
    recv_empty: BoolField,     // 5
    error: BoolField,          // 6
    data_8bit: BoolField,      // 7
    fifo_enabled: BoolField,   // 8
    parity_enabled: BoolField, // 9
    tx_enabled: BoolField,     // 10
    rx_enabled: BoolField,     // 11
    mode: SioMode,             // 12-13
    irq_enable: BoolField,     // 14
}

impl SioControlReg {
    fn default_uart() -> Self {
        Self {
            baud_rate: BaudRate::B9600,
            flow_control: BoolField(false),
            parity_odd: BoolField(false),
            send_full: BoolField(false),
            recv_empty: BoolField(false),
            error: BoolField(false),
            data_8bit: BoolField(true),
            // fifo_enabled: BoolField(true),
            fifo_enabled: BoolField(true),
            parity_enabled: BoolField(false),
            tx_enabled: BoolField(true),
            rx_enabled: BoolField(true),
            mode: SioMode::Uart,
            irq_enable: BoolField(false),
        }
    }

    fn with_baud(mut self, rate: BaudRate) -> Self {
        self.baud_rate = rate;
        self
    }

    fn with_interrupts(mut self, interrupts: bool) -> Self {
        *self.irq_enable = interrupts;
        self
    }

    fn with_cts(mut self, clear_to_send: bool) -> Self {
        *self.flow_control = clear_to_send;
        self
    }
}

impl From<SioControlReg> for u16 {
    fn from(value: SioControlReg) -> Self {
        value.baud_rate as u16
            | u16::from(value.flow_control) << 2
            | u16::from(value.parity_odd) << 3
            | u16::from(value.send_full) << 4
            | u16::from(value.recv_empty) << 5
            | u16::from(value.error) << 6
            | u16::from(value.data_8bit) << 7 // bit start
            | u16::from(value.fifo_enabled) << 8
            | u16::from(value.parity_enabled) << 9
            | u16::from(value.tx_enabled) << 10
            | u16::from(value.rx_enabled) << 11
            | (value.mode as u16) << 12
            | u16::from(value.irq_enable) << 14
    }
}

impl From<u16> for SioControlReg {
    fn from(value: u16) -> Self {
        Self {
            baud_rate: BaudRate::from(value & 0b11),
            flow_control: (value & (1 << 2)).into(),
            parity_odd: (value & (1 << 3)).into(),
            send_full: (value & (1 << 4)).into(),
            recv_empty: (value & (1 << 5)).into(),
            error: (value & (1 << 6)).into(),
            data_8bit: (value & (1 << 7)).into(),
            fifo_enabled: (value & (1 << 8)).into(),
            parity_enabled: (value & (1 << 9)).into(),
            tx_enabled: (value & (1 << 10)).into(),
            rx_enabled: (value & (1 << 11)).into(),
            mode: ((value & (0b11 << 12)) >> 12).into(),
            irq_enable: (value & (1 << 14)).into(),
        }
    }
}

pub struct BoolField(bool);

impl Deref for BoolField {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BoolField {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<BoolField> for u16 {
    fn from(value: BoolField) -> Self {
        if *value {
            1
        } else {
            0
        }
    }
}

impl From<u16> for BoolField {
    fn from(value: u16) -> Self {
        Self(value != 0)
    }
}

impl From<bool> for BoolField {
    fn from(value: bool) -> Self {
        Self(value)
    }
}
