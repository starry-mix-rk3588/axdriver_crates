//! RTL8139 Fast Ethernet driver implementation

use super::kernel_func::UseKernelFunc as KF;
use super::regs::rtl8139::*;
use crate::{EthernetAddress, NetBufPtr, NetDriverOps};
use axdriver_base::{BaseDriverOps, DevError, DevResult, DeviceType};
use core::ptr::NonNull;

/// RTL8139 constants
pub const NUM_TX_DESC: usize = 4;
pub const TX_BUF_SIZE: usize = 2048;
pub const RX_BUF_SIZE: usize = 8192 + 16 + 1536; // 8KB + margin
const MIN_ETH_FRAME_SIZE: usize = 60;

/// RTL8139 Fast Ethernet Driver
pub struct Rtl8139Driver {
    base_addr: usize,
    irq: u8,
    mac: EthernetAddress,

    // Transmit state
    tx_cur: usize,
    tx_buf_vaddr: [usize; NUM_TX_DESC],
    tx_buf_paddr: [usize; NUM_TX_DESC],

    // Receive state
    rx_buf_vaddr: usize,
    rx_buf_paddr: usize,
    cur_rx: usize,
}

impl Rtl8139Driver {
    /// Create a new RTL8139 driver instance
    pub fn new(base_addr: usize, irq: u8) -> DevResult<Self> {
        Ok(Self {
            base_addr,
            irq,
            mac: EthernetAddress([0; 6]),
            tx_cur: 0,
            tx_buf_vaddr: [0; NUM_TX_DESC],
            tx_buf_paddr: [0; NUM_TX_DESC],
            rx_buf_vaddr: 0,
            rx_buf_paddr: 0,
            cur_rx: 0,
        })
    }

    /// Initialize the RTL8139 hardware
    pub fn init(&mut self) -> DevResult {
        log::info!("Initializing RTL8139 at {:#x}", self.base_addr);

        // Power on the device
        self.write_reg8(CONFIG1, 0x00);

        // Software reset
        self.software_reset()?;

        // Allocate receive buffer
        let rx_pages = (RX_BUF_SIZE + 4095) / 4096;
        let (rx_vaddr, rx_paddr) = KF::dma_alloc_coherent(rx_pages);
        if rx_vaddr == 0 {
            log::error!("Failed to allocate RX buffer");
            return Err(DevError::NoMemory);
        }
        self.rx_buf_vaddr = rx_vaddr;
        self.rx_buf_paddr = rx_paddr;

        // Allocate transmit buffers
        let tx_pages = (TX_BUF_SIZE + 4095) / 4096;
        for i in 0..NUM_TX_DESC {
            let (tx_vaddr, tx_paddr) = KF::dma_alloc_coherent(tx_pages);
            if tx_vaddr == 0 {
                log::error!("Failed to allocate TX buffer {}", i);
                return Err(DevError::NoMemory);
            }
            self.tx_buf_vaddr[i] = tx_vaddr;
            self.tx_buf_paddr[i] = tx_paddr;

            // Set transmit address in TSAD register
            self.write_reg32(TSAD0 + (i as u16 * 4), tx_paddr as u32);
        }

        // Set receive buffer address
        self.write_reg32(RBSTART, self.rx_buf_paddr as u32);

        // Configure interrupts
        self.write_reg16(IMR, INT_ROK | INT_TOK);

        // Configure receive: Accept all packets with wrap
        self.write_reg32(RCR, 0xF | (1 << 7));

        // Configure transmit: Max DMA burst size + normal interframe gap
        self.write_reg32(TCR, 0x03000700);

        // Enable receiver and transmitter
        self.write_reg8(CR, CR_RE | CR_TE);

        // Read MAC address
        self.read_mac_address();

        log::info!(
            "RTL8139 initialized with MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
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
        self.write_reg8(CR, CR_RST);

        let mut timeout = 1000;
        while (self.read_reg8(CR) & CR_RST) != 0 {
            if timeout == 0 {
                log::error!("RTL8139 reset timeout");
                return Err(DevError::BadState);
            }
            timeout -= 1;
            KF::busy_wait_us(10);
        }

        Ok(())
    }

    /// Read MAC address from device
    fn read_mac_address(&mut self) {
        let mac_low = self.read_reg32(IDR0);
        let mac_high = self.read_reg16(IDR4);

        self.mac.0[0] = (mac_low & 0xFF) as u8;
        self.mac.0[1] = ((mac_low >> 8) & 0xFF) as u8;
        self.mac.0[2] = ((mac_low >> 16) & 0xFF) as u8;
        self.mac.0[3] = ((mac_low >> 24) & 0xFF) as u8;
        self.mac.0[4] = (mac_high & 0xFF) as u8;
        self.mac.0[5] = ((mac_high >> 8) & 0xFF) as u8;
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
        if data.len() > TX_BUF_SIZE {
            return Err(DevError::InvalidParam);
        }

        let idx = self.tx_cur;

        // Check if descriptor is available
        let status = self.read_reg32(TSD0 + (idx as u16 * 4));
        if (status & (TSD_OWN | TSD_TOK)) == 0 {
            // Not completed yet
            return Err(DevError::Again);
        }

        // Copy data to TX buffer
        let tx_buf = self.tx_buf_vaddr[idx];
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), tx_buf as *mut u8, data.len());
        }

        // Pad to minimum Ethernet frame size if needed
        let len = core::cmp::max(data.len(), MIN_ETH_FRAME_SIZE);

        // Start transmission by writing length to TSD register
        self.write_reg32(TSD0 + (idx as u16 * 4), len as u32);

        // Move to next descriptor
        self.tx_cur = (self.tx_cur + 1) % NUM_TX_DESC;

        Ok(())
    }

    /// Receive a packet
    fn do_receive(&mut self) -> DevResult<NetBufPtr> {
        // Check if buffer is empty
        if (self.read_reg8(CR) & CR_BUFE) != 0 {
            return Err(DevError::Again);
        }

        let rx_buf = self.rx_buf_vaddr;
        let cur_rx = self.cur_rx;

        unsafe {
            // Read packet status and length (first 4 bytes)
            let header_ptr = (rx_buf + cur_rx) as *const u32;
            let header = core::ptr::read_volatile(header_ptr);
            let status = (header & 0xFFFF) as u16;
            let total_len = (header >> 16) as usize;

            // Check receive OK status
            if (status & 0x01) == 0 {
                // Invalid packet, skip it
                self.cur_rx = (cur_rx + total_len + 4 + 3) & !3;
                self.write_reg16(CAPR, (self.cur_rx as u16).wrapping_sub(16));
                return Err(DevError::Again);
            }

            // Validate packet length (excluding 4-byte CRC)
            if total_len < 64 || total_len > 1522 {
                log::warn!("Invalid packet length: {}", total_len);
                self.cur_rx = (cur_rx + total_len + 4 + 3) & !3;
                self.write_reg16(CAPR, (self.cur_rx as u16).wrapping_sub(16));
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

            // Copy packet data (skip 4-byte header)
            core::ptr::copy_nonoverlapping(
                (rx_buf + cur_rx + 4) as *const u8,
                pkt_vaddr as *mut u8,
                packet_len,
            );

            // Update cur_rx pointer (4-byte aligned)
            self.cur_rx = (cur_rx + total_len + 4 + 3) & !3;

            // Wrap around if needed
            if self.cur_rx >= RX_BUF_SIZE {
                self.cur_rx = self.cur_rx % RX_BUF_SIZE;
            }

            // Update CAPR register (minus 16 to avoid overflow)
            self.write_reg16(CAPR, (self.cur_rx as u16).wrapping_sub(16));

            let raw_ptr = NonNull::new(pkt_vaddr as *mut u8).unwrap();
            let buf_ptr = raw_ptr;

            Ok(NetBufPtr::new(raw_ptr, buf_ptr, packet_len))
        }
    }
}

impl BaseDriverOps for Rtl8139Driver {
    fn device_name(&self) -> &str {
        "rtl8139"
    }

    fn device_type(&self) -> DeviceType {
        DeviceType::Net
    }
}

impl NetDriverOps for Rtl8139Driver {
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
        1
    }

    fn tx_queue_size(&self) -> usize {
        NUM_TX_DESC
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
        if size > TX_BUF_SIZE {
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

impl Drop for Rtl8139Driver {
    fn drop(&mut self) {
        // Disable receiver and transmitter
        self.write_reg8(CR, 0x00);

        // Free receive buffer
        if self.rx_buf_vaddr != 0 {
            let pages = (RX_BUF_SIZE + 4095) / 4096;
            KF::dma_free_coherent(self.rx_buf_vaddr, pages);
        }

        // Free transmit buffers
        let tx_pages = (TX_BUF_SIZE + 4095) / 4096;
        for i in 0..NUM_TX_DESC {
            if self.tx_buf_vaddr[i] != 0 {
                KF::dma_free_coherent(self.tx_buf_vaddr[i], tx_pages);
            }
        }
    }
}
