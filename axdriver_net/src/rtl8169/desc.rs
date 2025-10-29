//! RTL8169 DMA descriptor structures

/// Descriptor status bits
pub mod desc_status {
    pub const OWN: u32 = 0x80000000; // Descriptor owned by NIC
    pub const EOR: u32 = 0x40000000; // End of ring
    pub const FS: u32 = 0x20000000; // First segment
    pub const LS: u32 = 0x10000000; // Last segment
}

/// TX descriptor structure (16 bytes)
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct TxDesc {
    pub status: u32,
    pub vlan_tag: u32,
    pub buf_addr_low: u32,
    pub buf_addr_high: u32,
}

impl TxDesc {
    /// Create new zeroed TX descriptor
    pub const fn new() -> Self {
        Self {
            status: 0,
            vlan_tag: 0,
            buf_addr_low: 0,
            buf_addr_high: 0,
        }
    }

    /// Check if descriptor is owned by DMA
    #[inline]
    pub fn is_owned(&self) -> bool {
        (self.status & desc_status::OWN) != 0
    }

    /// Setup TX descriptor for transmission
    pub fn setup_tx(&mut self, buf_paddr: u32, len: u32, is_last: bool) {
        self.buf_addr_low = buf_paddr;
        self.buf_addr_high = 0;
        self.vlan_tag = 0;

        let mut status = desc_status::OWN | desc_status::FS | desc_status::LS;
        if is_last {
            status |= desc_status::EOR;
        }
        status |= len & 0xFFFF;

        self.status = status;
    }

    /// Clear descriptor
    pub fn clear(&mut self) {
        self.status = 0;
        self.vlan_tag = 0;
        self.buf_addr_low = 0;
        self.buf_addr_high = 0;
    }
}

/// RX descriptor structure (16 bytes)
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct RxDesc {
    pub status: u32,
    pub vlan_tag: u32,
    pub buf_addr_low: u32,
    pub buf_addr_high: u32,
}

/// RX status bits
pub mod rx_status {
    pub const RX_RES: u32 = 0x00200000;
    pub const RX_CRC: u32 = 0x00080000;
    pub const RX_RUNT: u32 = 0x00100000;
    pub const RX_RWT: u32 = 0x00400000;
}

impl RxDesc {
    /// Create new zeroed RX descriptor
    pub const fn new() -> Self {
        Self {
            status: 0,
            vlan_tag: 0,
            buf_addr_low: 0,
            buf_addr_high: 0,
        }
    }

    /// Check if descriptor is owned by DMA
    #[inline]
    pub fn is_owned(&self) -> bool {
        (self.status & desc_status::OWN) != 0
    }

    /// Setup RX descriptor for reception
    pub fn setup_rx(&mut self, buf_paddr: u32, buf_size: u32, is_last: bool) {
        self.buf_addr_low = buf_paddr;
        self.buf_addr_high = 0;
        self.vlan_tag = 0;

        let mut status = desc_status::OWN;
        if is_last {
            status |= desc_status::EOR;
        }
        status |= buf_size & 0xFFFF;

        self.status = status;
    }

    /// Get received packet length (only valid when OWN=0 and no error)
    pub fn get_rx_length(&self) -> Option<usize> {
        if self.is_owned() {
            return None;
        }

        // Check for errors
        if (self.status
            & (rx_status::RX_RES | rx_status::RX_CRC | rx_status::RX_RUNT | rx_status::RX_RWT))
            != 0
        {
            return None;
        }

        // Extract length (bits 0-12), subtract 4 for CRC
        let len = (self.status & 0x1FFF) as usize;
        if len >= 4 { Some(len - 4) } else { None }
    }

    /// Clear descriptor
    pub fn clear(&mut self) {
        self.status = 0;
        self.vlan_tag = 0;
        self.buf_addr_low = 0;
        self.buf_addr_high = 0;
    }
}
