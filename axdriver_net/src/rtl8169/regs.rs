//! RTL8169 register definitions and MMIO access wrapper
//!
//! Based on RealTek RTL8169/8110 Gigabit Ethernet controller specifications

use core::ptr::{read_volatile, write_volatile};

/// RTL8169 register offsets
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum Reg {
    // MAC address registers
    Mac0                 = 0x00,
    Mac4                 = 0x04,

    // Multicast filter
    Mar0                 = 0x08,
    Mar4                 = 0x0C,

    // Descriptor addresses
    TxDescStartAddrLow   = 0x20,
    TxDescStartAddrHigh  = 0x24,
    TxHDescStartAddrLow  = 0x28,
    TxHDescStartAddrHigh = 0x2C,

    // Flash memory interface
    Flash                = 0x30,

    // Early Rx Status
    ErSr                 = 0x36,

    // Command register
    ChipCmd              = 0x37,

    // TX poll
    TxPoll               = 0x38,

    // Interrupt mask/status
    IntrMask             = 0x3C,
    IntrStatus           = 0x3E,

    // TX/RX configuration
    TxConfig             = 0x40,
    RxConfig             = 0x44,

    // RX missed packet counter
    RxMissed             = 0x4C,

    // 93C46 command register
    Cfg9346              = 0x50,

    // Configuration registers
    Config0              = 0x51,
    Config1              = 0x52,
    Config2              = 0x53,
    Config3              = 0x54,
    Config4              = 0x55,
    Config5              = 0x56,

    // Multiple interrupt select
    MultiIntr            = 0x5C,

    // PHY access register
    PhyAr                = 0x60,

    // TBI control and status
    TbiCsr               = 0x64,
    TbiAnar              = 0x68,
    TbiLpar              = 0x6A,

    // PHY status
    PhyStatus            = 0x6C,

    // RX maximum size
    RxMaxSize            = 0xDA,

    // C+ command
    CPlusCmd             = 0xE0,

    // RX descriptor start address
    RxDescStartAddrLow   = 0xE4,
    RxDescStartAddrHigh  = 0xE8,

    // Early TX threshold
    EarlyTxThres         = 0xEC,

    // Function event/mask/state
    FuncEvent            = 0xF0,
    FuncEventMask        = 0xF4,
    FuncPresetState      = 0xF8,
    FuncForceEvent       = 0xFC,
}

/// ChipCmd register bits
#[allow(dead_code)]
pub mod chip_cmd {
    pub const CMD_RESET: u8 = 0x10;
    pub const CMD_RX_ENB: u8 = 0x08;
    pub const CMD_TX_ENB: u8 = 0x04;
    pub const RX_BUF_EMPTY: u8 = 0x01;
}

/// Cfg9346 register bits
#[allow(dead_code)]
pub mod cfg9346 {
    pub const LOCK: u8 = 0x00;
    pub const UNLOCK: u8 = 0xC0;
}

/// RX mode bits
#[allow(dead_code)]
pub mod rx_mode {
    pub const ACCEPT_ERR: u32 = 0x20;
    pub const ACCEPT_RUNT: u32 = 0x10;
    pub const ACCEPT_BROADCAST: u32 = 0x08;
    pub const ACCEPT_MULTICAST: u32 = 0x04;
    pub const ACCEPT_MY_PHYS: u32 = 0x02;
    pub const ACCEPT_ALL_PHYS: u32 = 0x01;
}

/// RX config register shifts
pub const RX_CFG_FIFO_SHIFT: u32 = 13;
pub const RX_CFG_DMA_SHIFT: u32 = 8;

/// TX config register shifts
pub const TX_INTERFRAME_GAP_SHIFT: u32 = 24;
pub const TX_DMA_SHIFT: u32 = 8;

/// DMA burst size
pub const RX_DMA_BURST: u32 = 6; // 1024 bytes
pub const TX_DMA_BURST: u32 = 6; // 1024 bytes

/// FIFO threshold
pub const RX_FIFO_THRESH: u32 = 7; // No threshold
pub const TX_FIFO_THRESH: u32 = 256;

/// Early TX threshold
pub const EARLY_TX_THLD: u8 = 0x3F;

/// Inter-frame gap
pub const INTER_FRAME_GAP: u32 = 0x03;

/// RX packet max size
pub const RX_PACKET_MAX_SIZE: u16 = 0x0800; // 2KB

/// PHY status register bits
#[allow(dead_code)]
pub mod phy_status {
    pub const TBI_ENABLE: u8 = 0x80;
    pub const TX_FLOW_CTRL: u8 = 0x40;
    pub const RX_FLOW_CTRL: u8 = 0x20;
    pub const _1000BPSF: u8 = 0x10;
    pub const _100BPS: u8 = 0x08;
    pub const _10BPS: u8 = 0x04;
    pub const LINK_STATUS: u8 = 0x02;
    pub const FULL_DUP: u8 = 0x01;
}

/// Interrupt status bits
#[allow(dead_code)]
pub mod intr_status {
    pub const SYS_ERR: u16 = 0x8000;
    pub const PCS_TIMEOUT: u16 = 0x4000;
    pub const SW_INT: u16 = 0x0100;
    pub const TX_DESC_UNAVAIL: u16 = 0x80;
    pub const RX_FIFO_OVER: u16 = 0x40;
    pub const RX_UNDERRUN: u16 = 0x20;
    pub const RX_OVERFLOW: u16 = 0x10;
    pub const TX_ERR: u16 = 0x08;
    pub const TX_OK: u16 = 0x04;
    pub const RX_ERR: u16 = 0x02;
    pub const RX_OK: u16 = 0x01;
}

/// RTL8169 chip information
#[derive(Debug, Clone, Copy)]
pub struct ChipInfo {
    pub name: &'static str,
    pub version: u8,
    pub rx_config_mask: u32,
}

/// Known RTL8169 chip versions
pub const CHIP_VERSIONS: &[ChipInfo] = &[
    ChipInfo {
        name: "RTL-8169",
        version: 0x00,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8169",
        version: 0x04,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8169s/8110s",
        version: 0x02,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8169s/8110s",
        version: 0x04,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8169sb/8110sb",
        version: 0x10,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8169sc/8110sc",
        version: 0x18,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8168b/8111sb",
        version: 0x30,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8168b/8111sb",
        version: 0x38,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8168c/8111c",
        version: 0x3c,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8168d/8111d",
        version: 0x28,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8168evl/8111evl",
        version: 0x2e,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8168/8111g",
        version: 0x4c,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8101e",
        version: 0x34,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8100e",
        version: 0x32,
        rx_config_mask: 0xff7e1880,
    },
    ChipInfo {
        name: "RTL-8168h/8111h",
        version: 0x54,
        rx_config_mask: 0xff7e1880,
    },
];

/// MMIO register access wrapper
pub struct Rtl8169Regs {
    base: usize,
}

impl Rtl8169Regs {
    /// Create new register accessor
    pub fn new(base: usize) -> Self {
        Self { base }
    }

    /// Read 8-bit register
    #[inline]
    pub fn read8(&self, reg: Reg) -> u8 {
        unsafe { read_volatile((self.base + reg as usize) as *const u8) }
    }

    /// Write 8-bit register
    #[inline]
    pub fn write8(&self, reg: Reg, val: u8) {
        unsafe { write_volatile((self.base + reg as usize) as *mut u8, val) }
    }

    /// Read 16-bit register
    #[inline]
    pub fn read16(&self, reg: Reg) -> u16 {
        unsafe { read_volatile((self.base + reg as usize) as *const u16) }
    }

    /// Write 16-bit register
    #[inline]
    pub fn write16(&self, reg: Reg, val: u16) {
        unsafe { write_volatile((self.base + reg as usize) as *mut u16, val) }
    }

    /// Read 32-bit register
    #[inline]
    pub fn read32(&self, reg: Reg) -> u32 {
        unsafe { read_volatile((self.base + reg as usize) as *const u32) }
    }

    /// Write 32-bit register
    #[inline]
    pub fn write32(&self, reg: Reg, val: u32) {
        unsafe { write_volatile((self.base + reg as usize) as *mut u32, val) }
    }
}
