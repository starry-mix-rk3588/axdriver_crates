//! DMA descriptor definitions for GMAC
//!
//! This module provides Enhanced DMA descriptor structures and methods
//! for the Synopsys DesignWare Ethernet MAC 4.20a.
//!
//! Both TX and RX descriptors are 32 bytes (8 x u32) with 8-byte alignment.
#![allow(unused)]
/// TX DMA Descriptor (Enhanced mode, 32 bytes)
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct TxDesc {
    pub tdes0: u32, // Status
    pub tdes1: u32, // Control and Buffer Sizes
    pub tdes2: u32, // Buffer 1 Address
    pub tdes3: u32, // Buffer 2 Address or Next Descriptor
    pub tdes4: u32, // Reserved
    pub tdes5: u32, // Reserved
    pub tdes6: u32, // Timestamp Low
    pub tdes7: u32, // Timestamp High
}

/// RX DMA Descriptor (Enhanced mode, 32 bytes)
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct RxDesc {
    pub rdes0: u32, // Status
    pub rdes1: u32, // Control and Buffer Sizes
    pub rdes2: u32, // Buffer 1 Address
    pub rdes3: u32, // Buffer 2 Address or Next Descriptor
    pub rdes4: u32, // Extended Status
    pub rdes5: u32, // Reserved
    pub rdes6: u32, // Timestamp Low
    pub rdes7: u32, // Timestamp High
}

impl TxDesc {
    /// TDES0 bit definitions - Status/Control
    pub const OWN: u32 = 1 << 31; // Descriptor owned by DMA
    pub const IC: u32 = 1 << 30;  // Interrupt on Completion
    pub const LS: u32 = 1 << 29;  // Last Segment
    pub const FS: u32 = 1 << 28;  // First Segment
    pub const DC: u32 = 1 << 27;  // Disable CRC
    pub const DP: u32 = 1 << 26;  // Disable Padding
    pub const TTSE: u32 = 1 << 25; // Transmit Timestamp Enable
    pub const CIC_SHIFT: u32 = 22; // Checksum Insertion Control (2 bits)
    pub const CIC_DISABLED: u32 = 0 << 22;
    pub const CIC_IP_ONLY: u32 = 1 << 22;
    pub const CIC_IP_PAYLOAD: u32 = 2 << 22;
    pub const CIC_IP_PSEUDO: u32 = 3 << 22;
    pub const TER: u32 = 1 << 21; // Transmit End of Ring
    pub const TCH: u32 = 1 << 20; // Second Address Chained
    pub const VLIC_SHIFT: u32 = 18; // VLAN Insertion Control (2 bits)
    pub const TTSS: u32 = 1 << 17; // Transmit Timestamp Status
    pub const IHE: u32 = 1 << 16;  // IP Header Error
    pub const ES: u32 = 1 << 15;   // Error Summary
    pub const JT: u32 = 1 << 14;   // Jabber Timeout
    pub const FF: u32 = 1 << 13;   // Frame Flushed
    pub const IPE: u32 = 1 << 12;  // IP Payload Error
    pub const LOC: u32 = 1 << 11;  // Loss of Carrier
    pub const NC: u32 = 1 << 10;   // No Carrier
    pub const LC: u32 = 1 << 9;    // Late Collision
    pub const EC: u32 = 1 << 8;    // Excessive Collision
    pub const VF: u32 = 1 << 7;    // VLAN Frame
    pub const CC_SHIFT: u32 = 3;   // Collision Count (4 bits)
    pub const CC_MASK: u32 = 0xF << 3;
    pub const ED: u32 = 1 << 2;    // Excessive Deferral
    pub const UF: u32 = 1 << 1;    // Underflow Error
    pub const DB: u32 = 1 << 0;    // Deferred Bit

    /// TDES1 bit definitions - Buffer sizes
    pub const TBS2_SHIFT: u32 = 16; // Transmit Buffer 2 Size (13 bits)
    pub const TBS2_MASK: u32 = 0x1FFF << 16;
    pub const TBS1_SHIFT: u32 = 0;  // Transmit Buffer 1 Size (13 bits)
    pub const TBS1_MASK: u32 = 0x1FFF;

    /// Create a new empty TX descriptor
    pub const fn new() -> Self {
        Self {
            tdes0: 0,
            tdes1: 0,
            tdes2: 0,
            tdes3: 0,
            tdes4: 0,
            tdes5: 0,
            tdes6: 0,
            tdes7: 0,
        }
    }

    /// Set buffer 1 address
    #[inline]
    pub fn set_buf1_addr(&mut self, addr: u32) {
        self.tdes2 = addr;
    }

    /// Set buffer 1 size
    #[inline]
    pub fn set_buf1_size(&mut self, size: u32) {
        self.tdes1 = (self.tdes1 & !Self::TBS1_MASK) | (size & Self::TBS1_MASK);
    }

    /// Set next descriptor address (for chained mode)
    #[inline]
    pub fn set_next_desc(&mut self, addr: u32) {
        self.tdes3 = addr;
    }

    /// Check if descriptor is owned by DMA
    #[inline]
    pub fn is_owned_by_dma(&self) -> bool {
        (self.tdes0 & Self::OWN) != 0
    }

    /// Set ownership to DMA
    #[inline]
    pub fn set_owned_by_dma(&mut self) {
        self.tdes0 |= Self::OWN;
    }

    /// Clear ownership (CPU owns)
    #[inline]
    pub fn clear_owned(&mut self) {
        self.tdes0 &= !Self::OWN;
    }

    /// Setup descriptor for transmission (chained mode, single buffer)
    #[inline]
    pub fn setup_tx(&mut self, buf_addr: u32, buf_size: u32, next_desc: u32, is_last: bool) {
        self.tdes0 = Self::FS | Self::LS | Self::TCH;
        if is_last {
            self.tdes0 |= Self::IC; // Interrupt on completion for last descriptor
        }
        self.tdes1 = buf_size & Self::TBS1_MASK;
        self.tdes2 = buf_addr;
        self.tdes3 = next_desc;
        self.tdes4 = 0;
        self.tdes5 = 0;
        self.tdes6 = 0;
        self.tdes7 = 0;
    }
}

impl RxDesc {
    /// RDES0 bit definitions - Status
    pub const OWN: u32 = 1 << 31;  // Descriptor owned by DMA
    pub const AFM: u32 = 1 << 30;  // Destination Address Filter Fail
    pub const FL_SHIFT: u32 = 16;  // Frame Length (14 bits)
    pub const FL_MASK: u32 = 0x3FFF << 16;
    pub const ES: u32 = 1 << 15;   // Error Summary
    pub const DE: u32 = 1 << 14;   // Descriptor Error
    pub const SAF: u32 = 1 << 13;  // Source Address Filter Fail
    pub const LE: u32 = 1 << 12;   // Length Error
    pub const OE: u32 = 1 << 11;   // Overflow Error
    pub const VLAN: u32 = 1 << 10; // VLAN Tag
    pub const FS: u32 = 1 << 9;    // First Descriptor
    pub const LS: u32 = 1 << 8;    // Last Descriptor
    pub const TSTAMP: u32 = 1 << 7; // Timestamp Available
    pub const LC: u32 = 1 << 6;    // Late Collision
    pub const FT: u32 = 1 << 5;    // Frame Type
    pub const RWT: u32 = 1 << 4;   // Receive Watchdog Timeout
    pub const RE: u32 = 1 << 3;    // Receive Error
    pub const DBE: u32 = 1 << 2;   // Dribble Bit Error
    pub const CE: u32 = 1 << 1;    // CRC Error
    pub const ESA: u32 = 1 << 0;   // Extended Status Available

    /// RDES1 bit definitions - Control and Buffer sizes
    pub const DIC: u32 = 1 << 31;  // Disable Interrupt on Completion
    pub const RBS2_SHIFT: u32 = 16; // Receive Buffer 2 Size (13 bits)
    pub const RBS2_MASK: u32 = 0x1FFF << 16;
    pub const RER: u32 = 1 << 15;  // Receive End of Ring
    pub const RCH: u32 = 1 << 14;  // Second Address Chained
    pub const RBS1_SHIFT: u32 = 0; // Receive Buffer 1 Size (13 bits)
    pub const RBS1_MASK: u32 = 0x1FFF;

    /// RDES4 bit definitions - Extended Status
    pub const IP_PAYLOAD_ERR: u32 = 1 << 7; // IP Payload Error
    pub const IP_HEADER_ERR: u32 = 1 << 3;  // IP Header Error
    pub const IP_PAYLOAD_TYPE_SHIFT: u32 = 5; // IP Payload Type (3 bits)

    /// Create a new empty RX descriptor
    pub const fn new() -> Self {
        Self {
            rdes0: 0,
            rdes1: 0,
            rdes2: 0,
            rdes3: 0,
            rdes4: 0,
            rdes5: 0,
            rdes6: 0,
            rdes7: 0,
        }
    }

    /// Set buffer 1 address
    #[inline]
    pub fn set_buf1_addr(&mut self, addr: u32) {
        self.rdes2 = addr;
    }

    /// Set buffer 1 size
    #[inline]
    pub fn set_buf1_size(&mut self, size: u32) {
        self.rdes1 = (self.rdes1 & !Self::RBS1_MASK) | (size & Self::RBS1_MASK);
    }

    /// Set next descriptor address (for chained mode)
    #[inline]
    pub fn set_next_desc(&mut self, addr: u32) {
        self.rdes3 = addr;
    }

    /// Check if descriptor is owned by DMA
    #[inline]
    pub fn is_owned_by_dma(&self) -> bool {
        (self.rdes0 & Self::OWN) != 0
    }

    /// Set ownership to DMA
    #[inline]
    pub fn set_owned_by_dma(&mut self) {
        self.rdes0 |= Self::OWN;
    }

    /// Clear ownership (CPU owns)
    #[inline]
    pub fn clear_owned(&mut self) {
        self.rdes0 &= !Self::OWN;
    }

    /// Check if frame has errors
    #[inline]
    pub fn has_error(&self) -> bool {
        (self.rdes0 & Self::ES) != 0
    }

    /// Get frame length (in bytes)
    #[inline]
    pub fn frame_length(&self) -> usize {
        ((self.rdes0 & Self::FL_MASK) >> Self::FL_SHIFT) as usize
    }

    /// Check if this is first and last descriptor (single buffer frame)
    #[inline]
    pub fn is_complete_frame(&self) -> bool {
        (self.rdes0 & (Self::FS | Self::LS)) == (Self::FS | Self::LS)
    }

    /// Setup descriptor for reception (chained mode, single buffer)
    #[inline]
    pub fn setup_rx(&mut self, buf_addr: u32, buf_size: u32, next_desc: u32) {
        self.rdes0 = Self::OWN;
        self.rdes1 = Self::RCH | (buf_size & Self::RBS1_MASK);
        self.rdes2 = buf_addr;
        self.rdes3 = next_desc;
        self.rdes4 = 0;
        self.rdes5 = 0;
        self.rdes6 = 0;
        self.rdes7 = 0;
    }
}

// Ensure correct size
const _: () = assert!(core::mem::size_of::<TxDesc>() == 32);
const _: () = assert!(core::mem::size_of::<RxDesc>() == 32);
const _: () = assert!(core::mem::align_of::<TxDesc>() == 8);
const _: () = assert!(core::mem::align_of::<RxDesc>() == 8);
