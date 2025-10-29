//! GMAC register definitions and access methods for RK3588
//!
//! This module provides register offsets, bit definitions, and safe access methods
//! for the Synopsys DesignWare Ethernet MAC 4.20a controller used in RK3588 SoC.
#![allow(unused)]
/// MAC Configuration Register offsets (Base + 0x0000)
pub const MAC_CONFIGURATION: usize = 0x0000;
pub const MAC_FRAME_FILTER: usize = 0x0004;
pub const MAC_HASH_TABLE_HIGH: usize = 0x0008;
pub const MAC_HASH_TABLE_LOW: usize = 0x000C;
pub const MAC_GMII_ADDRESS: usize = 0x0010;
pub const MAC_GMII_DATA: usize = 0x0014;
pub const MAC_FLOW_CONTROL: usize = 0x0018;
pub const MAC_VLAN_TAG: usize = 0x001C;
pub const MAC_VERSION: usize = 0x0020;
pub const MAC_DEBUG: usize = 0x0024;
pub const MAC_INTERRUPT_STATUS: usize = 0x0038;
pub const MAC_INTERRUPT_MASK: usize = 0x003C;
pub const MAC_ADDRESS0_HIGH: usize = 0x0040;
pub const MAC_ADDRESS0_LOW: usize = 0x0044;

/// DMA Register offsets (Base + 0x1000)
pub const DMA_BUS_MODE: usize = 0x1000;
pub const DMA_TX_POLL_DEMAND: usize = 0x1004;
pub const DMA_RX_POLL_DEMAND: usize = 0x1008;
pub const DMA_RX_DESC_LIST_ADDR: usize = 0x100C;
pub const DMA_TX_DESC_LIST_ADDR: usize = 0x1010;
pub const DMA_STATUS: usize = 0x1014;
pub const DMA_OPERATION_MODE: usize = 0x1018;
pub const DMA_INTERRUPT_ENABLE: usize = 0x101C;
pub const DMA_MISSED_FRAME_COUNTER: usize = 0x1020;
pub const DMA_CURRENT_HOST_TX_DESC: usize = 0x1048;
pub const DMA_CURRENT_HOST_RX_DESC: usize = 0x104C;
pub const DMA_CURRENT_HOST_TX_BUFFER: usize = 0x1050;
pub const DMA_CURRENT_HOST_RX_BUFFER: usize = 0x1054;

/// MAC_CONFIGURATION register bit definitions
pub const MAC_CONFIG_RE: u32 = 1 << 2;     // Receiver Enable
pub const MAC_CONFIG_TE: u32 = 1 << 3;     // Transmitter Enable
pub const MAC_CONFIG_DC: u32 = 1 << 4;     // Deferral Check
pub const MAC_CONFIG_BL_10: u32 = 0 << 5;  // Back-Off Limit: 10
pub const MAC_CONFIG_ACS: u32 = 1 << 7;    // Automatic Pad/CRC Stripping
pub const MAC_CONFIG_DR: u32 = 1 << 9;     // Disable Retry
pub const MAC_CONFIG_IPC: u32 = 1 << 10;   // Checksum Offload
pub const MAC_CONFIG_DM: u32 = 1 << 11;    // Duplex Mode (1=Full, 0=Half)
pub const MAC_CONFIG_LM: u32 = 1 << 12;    // Loopback Mode
pub const MAC_CONFIG_DO: u32 = 1 << 13;    // Disable Receive Own
pub const MAC_CONFIG_FES: u32 = 1 << 14;   // Speed (1=100Mbps, 0=10Mbps)
pub const MAC_CONFIG_PS: u32 = 1 << 15;    // Port Select (1=MII, 0=GMII)
pub const MAC_CONFIG_DCRS: u32 = 1 << 16;  // Disable Carrier Sense
pub const MAC_CONFIG_IFG_96: u32 = 0 << 17;// Inter-Frame Gap: 96 bit times
pub const MAC_CONFIG_JE: u32 = 1 << 20;    // Jumbo Frame Enable
pub const MAC_CONFIG_JD: u32 = 1 << 22;    // Jabber Disable
pub const MAC_CONFIG_WD: u32 = 1 << 23;    // Watchdog Disable
pub const MAC_CONFIG_TC: u32 = 1 << 24;    // Transmit Configuration in RGMII
pub const MAC_CONFIG_CST: u32 = 1 << 25;   // CRC Stripping for Type frames

/// MAC_FRAME_FILTER register bit definitions
pub const MAC_FILTER_PR: u32 = 1 << 0;     // Promiscuous Mode
pub const MAC_FILTER_HUC: u32 = 1 << 1;    // Hash Unicast
pub const MAC_FILTER_HMC: u32 = 1 << 2;    // Hash Multicast
pub const MAC_FILTER_DAIF: u32 = 1 << 3;   // DA Inverse Filtering
pub const MAC_FILTER_PM: u32 = 1 << 4;     // Pass All Multicast
pub const MAC_FILTER_DBF: u32 = 1 << 5;    // Disable Broadcast Frames
pub const MAC_FILTER_PCF_NONE: u32 = 0 << 6; // Pass Control Frames: None
pub const MAC_FILTER_SAIF: u32 = 1 << 8;   // SA Inverse Filtering
pub const MAC_FILTER_SAF: u32 = 1 << 9;    // Source Address Filter
pub const MAC_FILTER_HPF: u32 = 1 << 10;   // Hash or Perfect Filter
pub const MAC_FILTER_RA: u32 = 1 << 31;    // Receive All

/// DMA_BUS_MODE register bit definitions
pub const DMA_BUS_MODE_SWR: u32 = 1 << 0;   // Software Reset
pub const DMA_BUS_MODE_DA: u32 = 1 << 1;    // DMA Arbitration (0=RR, 1=TX>RX)
pub const DMA_BUS_MODE_DSL_MASK: u32 = 0x1F << 2; // Descriptor Skip Length
pub const DMA_BUS_MODE_ATDS: u32 = 1 << 7;  // Alternate Descriptor Size
pub const DMA_BUS_MODE_PBL_1: u32 = 1 << 8; // Programmable Burst Length: 1
pub const DMA_BUS_MODE_PBL_2: u32 = 2 << 8;
pub const DMA_BUS_MODE_PBL_4: u32 = 4 << 8;
pub const DMA_BUS_MODE_PBL_8: u32 = 8 << 8;
pub const DMA_BUS_MODE_PBL_16: u32 = 16 << 8;
pub const DMA_BUS_MODE_PBL_32: u32 = 32 << 8;
pub const DMA_BUS_MODE_FB: u32 = 1 << 16;   // Fixed Burst
pub const DMA_BUS_MODE_RPBL_1: u32 = 1 << 17; // RX DMA PBL
pub const DMA_BUS_MODE_USP: u32 = 1 << 23;  // Use Separate PBL
pub const DMA_BUS_MODE_8XPBL: u32 = 1 << 24; // 8x PBL Mode
pub const DMA_BUS_MODE_AAL: u32 = 1 << 25;  // Address-Aligned Beats
pub const DMA_BUS_MODE_MB: u32 = 1 << 26;   // Mixed Burst
pub const DMA_BUS_MODE_TXPR: u32 = 1 << 27; // Transmit Priority

/// DMA_STATUS register bit definitions
pub const DMA_STATUS_TI: u32 = 1 << 0;      // Transmit Interrupt
pub const DMA_STATUS_TPS: u32 = 1 << 1;     // Transmit Process Stopped
pub const DMA_STATUS_TU: u32 = 1 << 2;      // Transmit Buffer Unavailable
pub const DMA_STATUS_TJT: u32 = 1 << 3;     // Transmit Jabber Timeout
pub const DMA_STATUS_OVF: u32 = 1 << 4;     // Receive Overflow
pub const DMA_STATUS_UNF: u32 = 1 << 5;     // Transmit Underflow
pub const DMA_STATUS_RI: u32 = 1 << 6;      // Receive Interrupt
pub const DMA_STATUS_RU: u32 = 1 << 7;      // Receive Buffer Unavailable
pub const DMA_STATUS_RPS: u32 = 1 << 8;     // Receive Process Stopped
pub const DMA_STATUS_RWT: u32 = 1 << 9;     // Receive Watchdog Timeout
pub const DMA_STATUS_ETI: u32 = 1 << 10;    // Early Transmit Interrupt
pub const DMA_STATUS_FBI: u32 = 1 << 13;    // Fatal Bus Error Interrupt
pub const DMA_STATUS_ERI: u32 = 1 << 14;    // Early Receive Interrupt
pub const DMA_STATUS_AIS: u32 = 1 << 15;    // Abnormal Interrupt Summary
pub const DMA_STATUS_NIS: u32 = 1 << 16;    // Normal Interrupt Summary
pub const DMA_STATUS_RS_SHIFT: u32 = 17;    // Receive Process State
pub const DMA_STATUS_TS_SHIFT: u32 = 20;    // Transmit Process State
pub const DMA_STATUS_EB_SHIFT: u32 = 23;    // Error Bits

/// DMA_OPERATION_MODE register bit definitions
pub const DMA_OP_MODE_SR: u32 = 1 << 1;     // Start/Stop Receive
pub const DMA_OP_MODE_OSF: u32 = 1 << 2;    // Operate on Second Frame
pub const DMA_OP_MODE_RTC_64: u32 = 0 << 3; // Receive Threshold Control: 64
pub const DMA_OP_MODE_RTC_32: u32 = 1 << 3;
pub const DMA_OP_MODE_RTC_96: u32 = 2 << 3;
pub const DMA_OP_MODE_RTC_128: u32 = 3 << 3;
pub const DMA_OP_MODE_FUF: u32 = 1 << 6;    // Forward Undersized Good Frames
pub const DMA_OP_MODE_FEF: u32 = 1 << 7;    // Forward Error Frames
pub const DMA_OP_MODE_ST: u32 = 1 << 13;    // Start/Stop Transmission
pub const DMA_OP_MODE_TTC_64: u32 = 0 << 14; // Transmit Threshold Control: 64
pub const DMA_OP_MODE_TTC_128: u32 = 1 << 14;
pub const DMA_OP_MODE_TTC_192: u32 = 2 << 14;
pub const DMA_OP_MODE_TTC_256: u32 = 3 << 14;
pub const DMA_OP_MODE_TTC_40: u32 = 4 << 14;
pub const DMA_OP_MODE_TTC_32: u32 = 5 << 14;
pub const DMA_OP_MODE_TTC_24: u32 = 6 << 14;
pub const DMA_OP_MODE_TTC_16: u32 = 7 << 14;
pub const DMA_OP_MODE_FTF: u32 = 1 << 20;   // Flush Transmit FIFO
pub const DMA_OP_MODE_TSF: u32 = 1 << 21;   // Transmit Store and Forward
pub const DMA_OP_MODE_DFF: u32 = 1 << 24;   // Disable Flushing of Received Frames
pub const DMA_OP_MODE_RSF: u32 = 1 << 25;   // Receive Store and Forward
pub const DMA_OP_MODE_DT: u32 = 1 << 26;    // Disable Dropping of TCP/IP Checksum Error Frames

/// DMA_INTERRUPT_ENABLE register bit definitions
pub const DMA_INT_TX_COMPLETED: u32 = 1 << 0;      // Transmit Interrupt Enable
pub const DMA_INT_TX_STOPPED: u32 = 1 << 1;        // Transmit Stopped Enable
pub const DMA_INT_TX_BUF_UNAVAIL: u32 = 1 << 2;    // Transmit Buffer Unavailable Enable
pub const DMA_INT_TX_JABBER: u32 = 1 << 3;         // Transmit Jabber Timeout Enable
pub const DMA_INT_RX_OVERFLOW: u32 = 1 << 4;       // Receive Overflow Enable
pub const DMA_INT_TX_UNDERFLOW: u32 = 1 << 5;      // Transmit Underflow Enable
pub const DMA_INT_RX_COMPLETED: u32 = 1 << 6;      // Receive Interrupt Enable
pub const DMA_INT_RX_BUF_UNAVAIL: u32 = 1 << 7;    // Receive Buffer Unavailable Enable
pub const DMA_INT_RX_STOPPED: u32 = 1 << 8;        // Receive Stopped Enable
pub const DMA_INT_RX_WATCHDOG: u32 = 1 << 9;       // Receive Watchdog Timeout Enable
pub const DMA_INT_EARLY_TX: u32 = 1 << 10;         // Early Transmit Interrupt Enable
pub const DMA_INT_FATAL_BUS_ERROR: u32 = 1 << 13;  // Fatal Bus Error Enable
pub const DMA_INT_EARLY_RX: u32 = 1 << 14;         // Early Receive Interrupt Enable
pub const DMA_INT_ABNORMAL: u32 = 1 << 15;         // Abnormal Interrupt Summary Enable
pub const DMA_INT_NORMAL: u32 = 1 << 16;           // Normal Interrupt Summary Enable

/// Register accessor with safe read/write methods
pub struct GmacRegs {
    pub base: usize,
}

impl GmacRegs {
    /// Create a new register accessor
    pub const fn new(base: usize) -> Self {
        Self { base }
    }

    /// Read a 32-bit register at the given offset
    #[inline]
    pub fn read_reg(&self, offset: usize) -> u32 {
        unsafe { core::ptr::read_volatile((self.base + offset) as *const u32) }
    }

    /// Write a 32-bit value to the register at the given offset
    #[inline]
    pub fn write_reg(&self, offset: usize, value: u32) {
        unsafe {
            core::ptr::write_volatile((self.base + offset) as *mut u32, value);
        }
    }

    /// Set specific bits in a register (read-modify-write)
    #[inline]
    pub fn set_bits(&self, offset: usize, mask: u32) {
        let value = self.read_reg(offset);
        self.write_reg(offset, value | mask);
    }

    /// Clear specific bits in a register (read-modify-write)
    #[inline]
    pub fn clear_bits(&self, offset: usize, mask: u32) {
        let value = self.read_reg(offset);
        self.write_reg(offset, value & !mask);
    }

    /// Modify bits in a register with a mask (read-modify-write)
    #[inline]
    pub fn modify_reg(&self, offset: usize, clear_mask: u32, set_mask: u32) {
        let value = self.read_reg(offset);
        self.write_reg(offset, (value & !clear_mask) | set_mask);
    }

    /// Read and return register value, useful for chaining
    #[inline]
    pub fn read_and_clear(&self, offset: usize) -> u32 {
        let value = self.read_reg(offset);
        self.write_reg(offset, value); // Write back to clear
        value
    }
}
