//! RTL8169/RTL8168/RTL8111 Gigabit Ethernet driver implementation

use super::device_info::RealtekSeries;
use super::kernel_func::UseKernelFunc as KF;
use super::regs::descriptor::*;
use super::regs::rtl8169::*;
use crate::{EthernetAddress, NetBufPtr, NetDriverOps};
use axdriver_base::{BaseDriverOps, DevError, DevResult, DeviceType};
use core::ptr::NonNull;

/// RTL8169 constants
pub const NUM_DESC: usize = 128;
pub const BUF_SIZE: usize = 2048;
const MIN_ETH_FRAME_SIZE: usize = 60;

/// RTL8169/8168 Descriptor
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct Descriptor {
    pub opts1: u32,
    pub opts2: u32,
    pub addr_low: u32,
    pub addr_high: u32,
}

impl Descriptor {
    pub const fn new() -> Self {
        Self {
            opts1: 0,
            opts2: 0,
            addr_low: 0,
            addr_high: 0,
        }
    }
}

/// RTL8169/RTL8168/RTL8111 Gigabit Ethernet Driver
pub struct Rtl8169Driver {
    base_addr: usize,
    irq: u8,
    mac: EthernetAddress,
    series: RealtekSeries,

    // Transmit ring
    tx_desc_vaddr: usize,
    tx_desc_paddr: usize,
    tx_buf_vaddr: [usize; NUM_DESC],
    tx_buf_paddr: [usize; NUM_DESC],
    tx_cur: usize,

    // Receive ring
    rx_desc_vaddr: usize,
    rx_desc_paddr: usize,
    rx_buf_vaddr: [usize; NUM_DESC],
    rx_buf_paddr: [usize; NUM_DESC],
    rx_cur: usize,
}

impl Rtl8169Driver {
    /// Create a new RTL8169/8168/8111 driver instance
    pub fn new(base_addr: usize, irq: u8, series: RealtekSeries) -> DevResult<Self> {
        Ok(Self {
            base_addr,
            irq,
            mac: EthernetAddress([0; 6]),
            series,
            tx_desc_vaddr: 0,
            tx_desc_paddr: 0,
            tx_buf_vaddr: [0; NUM_DESC],
            tx_buf_paddr: [0; NUM_DESC],
            tx_cur: 0,
            rx_desc_vaddr: 0,
            rx_desc_paddr: 0,
            rx_buf_vaddr: [0; NUM_DESC],
            rx_buf_paddr: [0; NUM_DESC],
            rx_cur: 0,
        })
    }

    /// Initialize the RTL8169/8168/8111 hardware
    pub fn init(&mut self) -> DevResult {
        log::info!("Initializing RTL8169/8168/8111 at {:#x}", self.base_addr);

        // Software reset
        self.software_reset()?;

        // Unlock configuration registers
        self.write_reg8(CFG_9346, CFG_9346_UNLOCK);

        // Allocate descriptor rings
        self.allocate_descriptors()?;

        // Allocate buffers and setup descriptors
        self.setup_tx_ring()?;
        self.setup_rx_ring()?;

        // Set descriptor ring addresses
        self.write_reg32(TNPDS_LO, self.tx_desc_paddr as u32);
        self.write_reg32(TNPDS_HI, (self.tx_desc_paddr >> 32) as u32);
        self.write_reg32(RDSAR_LO, self.rx_desc_paddr as u32);
        self.write_reg32(RDSAR_HI, (self.rx_desc_paddr >> 32) as u32);

        // Configure receive
        let rcr_val = RCR_AAP | RCR_APM | RCR_AM | RCR_AB | RCR_MXDMA_UNLIMITED | RCR_RXFTH_NONE;
        self.write_reg32(RCR, rcr_val);

        // Configure transmit
        let tcr_val = TCR_MXDMA_UNLIMITED | TCR_IFG_NORMAL;
        self.write_reg32(TCR, tcr_val);

        // Set max receive packet size
        self.write_reg16(RMS, BUF_SIZE as u16);

        // Configure interrupts
        self.write_reg16(IMR, INT_ROK | INT_TOK | INT_RER | INT_TER);

        // Enable receiver and transmitter
        self.write_reg8(CMD, CMD_RE | CMD_TE);

        // Lock configuration registers
        self.write_reg8(CFG_9346, CFG_9346_LOCK);

        // Read MAC address
        self.read_mac_address();

        log::info!(
            "RTL8169/8168/8111 initialized with MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac.0[0],
            self.mac.0[1],
            self.mac.0[2],
            self.mac.0[3],
            self.mac.0[4],
            self.mac.0[5]
        );

        Ok(())
    }

    /// Perform software reset
    fn software_reset(&self) -> DevResult {
        self.write_reg8(CMD, CMD_RST);

        let mut timeout = 1000;
        while (self.read_reg8(CMD) & CMD_RST) != 0 {
            if timeout == 0 {
                log::error!("RTL8169 reset timeout");
                return Err(DevError::BadState);
            }
            timeout -= 1;
            KF::busy_wait_us(10);
        }

        Ok(())
    }

    /// Allocate descriptor rings
    fn allocate_descriptors(&mut self) -> DevResult {
        let desc_size = core::mem::size_of::<Descriptor>() * NUM_DESC;
        let desc_pages = (desc_size + 4095) / 4096;

        // Allocate TX descriptor ring
        let (tx_desc_vaddr, tx_desc_paddr) = KF::dma_alloc_coherent(desc_pages);
        if tx_desc_vaddr == 0 {
            log::error!("Failed to allocate TX descriptors");
            return Err(DevError::NoMemory);
        }
        self.tx_desc_vaddr = tx_desc_vaddr;
        self.tx_desc_paddr = tx_desc_paddr;

        // Allocate RX descriptor ring
        let (rx_desc_vaddr, rx_desc_paddr) = KF::dma_alloc_coherent(desc_pages);
        if rx_desc_vaddr == 0 {
            log::error!("Failed to allocate RX descriptors");
            return Err(DevError::NoMemory);
        }
        self.rx_desc_vaddr = rx_desc_vaddr;
        self.rx_desc_paddr = rx_desc_paddr;

        Ok(())
    }

    /// Setup transmit ring
    fn setup_tx_ring(&mut self) -> DevResult {
        let buf_pages = (BUF_SIZE + 4095) / 4096;

        for i in 0..NUM_DESC {
            // Allocate TX buffer
            let (tx_buf_vaddr, tx_buf_paddr) = KF::dma_alloc_coherent(buf_pages);
            if tx_buf_vaddr == 0 {
                log::error!("Failed to allocate TX buffer {}", i);
                return Err(DevError::NoMemory);
            }
            self.tx_buf_vaddr[i] = tx_buf_vaddr;
            self.tx_buf_paddr[i] = tx_buf_paddr;

            // Setup descriptor
            let desc_addr = self.tx_desc_vaddr + i * core::mem::size_of::<Descriptor>();
            let desc = unsafe { &mut *(desc_addr as *mut Descriptor) };
            desc.addr_low = tx_buf_paddr as u32;
            desc.addr_high = (tx_buf_paddr >> 32) as u32;
            desc.opts1 = if i == NUM_DESC - 1 { TX_EOR } else { 0 };
            desc.opts2 = 0;
        }

        Ok(())
    }

    /// Setup receive ring
    fn setup_rx_ring(&mut self) -> DevResult {
        let buf_pages = (BUF_SIZE + 4095) / 4096;

        for i in 0..NUM_DESC {
            // Allocate RX buffer
            let (rx_buf_vaddr, rx_buf_paddr) = KF::dma_alloc_coherent(buf_pages);
            if rx_buf_vaddr == 0 {
                log::error!("Failed to allocate RX buffer {}", i);
                return Err(DevError::NoMemory);
            }
            self.rx_buf_vaddr[i] = rx_buf_vaddr;
            self.rx_buf_paddr[i] = rx_buf_paddr;

            // Setup descriptor
            let desc_addr = self.rx_desc_vaddr + i * core::mem::size_of::<Descriptor>();
            let desc = unsafe { &mut *(desc_addr as *mut Descriptor) };
            desc.addr_low = rx_buf_paddr as u32;
            desc.addr_high = (rx_buf_paddr >> 32) as u32;
            let eor = if i == NUM_DESC - 1 { RX_EOR } else { 0 };
            desc.opts1 = RX_OWN | eor | (BUF_SIZE as u32 & RX_LEN_MASK);
            desc.opts2 = 0;
        }

        Ok(())
    }

    /// Read MAC address from device
    fn read_mac_address(&mut self) {
        for i in 0..6 {
            self.mac.0[i] = self.read_reg8(MAC0 + i as u16);
        }
    }

    /// Read 8-bit register
    #[inline]
    fn read_reg8(&self, offset: u16) -> u8 {
        unsafe { core::ptr::read_volatile((self.base_addr + offset as usize) as *const u8) }
    }

    /// Write 8-bit register
    #[inline]
    fn write_reg8(&self, offset: u16, value: u8) {
        unsafe {
            core::ptr::write_volatile((self.base_addr + offset as usize) as *mut u8, value);
        }
    }

    /// Read 16-bit register
    #[inline]
    fn read_reg16(&self, offset: u16) -> u16 {
        unsafe { core::ptr::read_volatile((self.base_addr + offset as usize) as *const u16) }
    }

    /// Write 16-bit register
    #[inline]
    fn write_reg16(&self, offset: u16, value: u16) {
        unsafe {
            core::ptr::write_volatile((self.base_addr + offset as usize) as *mut u16, value);
        }
    }

    /// Read 32-bit register
    #[inline]
    fn read_reg32(&self, offset: u16) -> u32 {
        unsafe { core::ptr::read_volatile((self.base_addr + offset as usize) as *const u32) }
    }

    /// Write 32-bit register
    #[inline]
    fn write_reg32(&self, offset: u16, value: u32) {
        unsafe {
            core::ptr::write_volatile((self.base_addr + offset as usize) as *mut u32, value);
        }
    }

    /// Transmit a packet
    fn do_transmit(&mut self, data: &[u8]) -> DevResult {
        if data.len() > BUF_SIZE {
            return Err(DevError::InvalidParam);
        }

        let idx = self.tx_cur;
        let desc_addr = self.tx_desc_vaddr + idx * core::mem::size_of::<Descriptor>();
        let desc = unsafe { &mut *(desc_addr as *mut Descriptor) };

        // Check if descriptor is available
        if (desc.opts1 & TX_OWN) != 0 {
            return Err(DevError::Again);
        }

        // Copy data to TX buffer
        let tx_buf = self.tx_buf_vaddr[idx];
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), tx_buf as *mut u8, data.len());
        }

        // Pad to minimum Ethernet frame size if needed
        let len = core::cmp::max(data.len(), MIN_ETH_FRAME_SIZE);

        // Setup descriptor
        let eor = if idx == NUM_DESC - 1 { TX_EOR } else { 0 };
        desc.opts1 = TX_OWN | TX_FS | TX_LS | eor | (len as u32 & TX_LEN_MASK);
        desc.opts2 = 0;

        // Memory barrier to ensure descriptor is written
        core::sync::atomic::fence(core::sync::atomic::Ordering::Release);

        // Trigger transmission
        self.write_reg8(TPPOLL, TPPOLL_NPQ);

        // Move to next descriptor
        self.tx_cur = (self.tx_cur + 1) % NUM_DESC;

        Ok(())
    }

    /// Receive a packet
    fn do_receive(&mut self) -> DevResult<NetBufPtr> {
        let idx = self.rx_cur;
        let desc_addr = self.rx_desc_vaddr + idx * core::mem::size_of::<Descriptor>();
        let desc = unsafe { &mut *(desc_addr as *mut Descriptor) };

        // Check if packet is available
        if (desc.opts1 & RX_OWN) != 0 {
            return Err(DevError::Again);
        }

        // Check for errors
        if (desc.opts1 & RX_RES) != 0 {
            log::warn!("RX error: opts1={:#x}", desc.opts1);
            // Reset descriptor
            let eor = if idx == NUM_DESC - 1 { RX_EOR } else { 0 };
            desc.opts1 = RX_OWN | eor | (BUF_SIZE as u32 & RX_LEN_MASK);
            self.rx_cur = (self.rx_cur + 1) % NUM_DESC;
            return Err(DevError::BadState);
        }

        // Get packet length (excluding CRC)
        let total_len = (desc.opts1 & RX_LEN_MASK) as usize;
        if total_len < 64 || total_len > BUF_SIZE {
            log::warn!("Invalid RX packet length: {}", total_len);
            // Reset descriptor
            let eor = if idx == NUM_DESC - 1 { RX_EOR } else { 0 };
            desc.opts1 = RX_OWN | eor | (BUF_SIZE as u32 & RX_LEN_MASK);
            self.rx_cur = (self.rx_cur + 1) % NUM_DESC;
            return Err(DevError::BadState);
        }

        // Packet length excluding CRC
        let packet_len = total_len - 4;

        // Allocate buffer for packet
        let pages = (packet_len + 4095) / 4096;
        let (pkt_vaddr, _pkt_paddr) = KF::dma_alloc_coherent(pages);
        if pkt_vaddr == 0 {
            log::error!("Failed to allocate packet buffer");
            return Err(DevError::NoMemory);
        }

        // Copy packet data
        let rx_buf = self.rx_buf_vaddr[idx];
        unsafe {
            core::ptr::copy_nonoverlapping(rx_buf as *const u8, pkt_vaddr as *mut u8, packet_len);
        }

        // Reset descriptor for next packet
        let eor = if idx == NUM_DESC - 1 { RX_EOR } else { 0 };
        desc.opts1 = RX_OWN | eor | (BUF_SIZE as u32 & RX_LEN_MASK);
        desc.opts2 = 0;

        // Move to next descriptor
        self.rx_cur = (self.rx_cur + 1) % NUM_DESC;

        let raw_ptr = NonNull::new(pkt_vaddr as *mut u8).unwrap();
        let buf_ptr = raw_ptr;

        Ok(NetBufPtr::new(raw_ptr, buf_ptr, packet_len))
    }
}

impl BaseDriverOps for Rtl8169Driver {
    fn device_name(&self) -> &str {
        match self.series {
            RealtekSeries::Rtl8169 => "rtl8169",
            RealtekSeries::Rtl8168 => "rtl8168",
            RealtekSeries::Rtl8111 => "rtl8111",
            _ => "rtl8169",
        }
    }

    fn device_type(&self) -> DeviceType {
        DeviceType::Net
    }
}

impl NetDriverOps for Rtl8169Driver {
    fn mac_address(&self) -> EthernetAddress {
        self.mac
    }

    fn can_transmit(&self) -> bool {
        true
    }

    fn can_receive(&self) -> bool {
        true
    }

    fn rx_queue_size(&self) -> usize {
        NUM_DESC
    }

    fn tx_queue_size(&self) -> usize {
        NUM_DESC
    }

    fn recycle_rx_buffer(&mut self, rx_buf: NetBufPtr) -> DevResult {
        let vaddr = rx_buf.raw_ptr::<u8>() as usize;
        let pages = (rx_buf.packet_len() + 4095) / 4096;
        KF::dma_free_coherent(vaddr, pages);
        Ok(())
    }

    fn recycle_tx_buffers(&mut self) -> DevResult {
        // TX buffers are reused in place
        Ok(())
    }

    fn transmit(&mut self, tx_buf: NetBufPtr) -> DevResult {
        let data = tx_buf.packet();
        let result = self.do_transmit(data);

        // Free the buffer after transmission attempt
        self.recycle_rx_buffer(tx_buf)?;
        result
    }

    fn receive(&mut self) -> DevResult<NetBufPtr> {
        self.do_receive()
    }

    fn alloc_tx_buffer(&mut self, size: usize) -> DevResult<NetBufPtr> {
        if size > BUF_SIZE {
            return Err(DevError::InvalidParam);
        }

        let pages = (size + 4095) / 4096;
        let (vaddr, _paddr) = KF::dma_alloc_coherent(pages);

        if vaddr == 0 {
            return Err(DevError::NoMemory);
        }

        let raw_ptr = NonNull::new(vaddr as *mut u8).unwrap();
        let buf_ptr = raw_ptr;

        Ok(NetBufPtr::new(raw_ptr, buf_ptr, size))
    }
}

impl Drop for Rtl8169Driver {
    fn drop(&mut self) {
        // Disable receiver and transmitter
        self.write_reg8(CMD, 0x00);

        // Free descriptor rings
        if self.tx_desc_vaddr != 0 {
            let desc_size = core::mem::size_of::<Descriptor>() * NUM_DESC;
            let pages = (desc_size + 4095) / 4096;
            KF::dma_free_coherent(self.tx_desc_vaddr, pages);
        }

        if self.rx_desc_vaddr != 0 {
            let desc_size = core::mem::size_of::<Descriptor>() * NUM_DESC;
            let pages = (desc_size + 4095) / 4096;
            KF::dma_free_coherent(self.rx_desc_vaddr, pages);
        }

        // Free buffers
        let buf_pages = (BUF_SIZE + 4095) / 4096;
        for i in 0..NUM_DESC {
            if self.tx_buf_vaddr[i] != 0 {
                KF::dma_free_coherent(self.tx_buf_vaddr[i], buf_pages);
            }
            if self.rx_buf_vaddr[i] != 0 {
                KF::dma_free_coherent(self.rx_buf_vaddr[i], buf_pages);
            }
        }
    }
}
